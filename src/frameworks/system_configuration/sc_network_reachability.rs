/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! SCNetworkReachability
//!
//! Reachability is never actually probed: if the app is allowed network access
//! (`--allow-network-access`), every target is reported as reachable, and
//! otherwise nothing is. The state of a target never changes while the app is
//! running, so callbacks are only ever delivered once, right after the target
//! is scheduled on a run loop (which is also what the real implementation
//! does, to tell the app the initial state).

use crate::abi::{CallFromHost, GuestFunction};
use crate::dyld::{export_c_func, FunctionExports};
use crate::frameworks::core_foundation::cf_allocator::{kCFAllocatorDefault, CFAllocatorRef};
use crate::frameworks::core_foundation::cf_run_loop::{CFRunLoopMode, CFRunLoopRef};
use crate::frameworks::core_foundation::{CFIndex, CFTypeRef};
use crate::libc::sys::socket::sockaddr;
use crate::mem::{ConstPtr, MutPtr, MutVoidPtr, SafeRead};
use crate::objc::{id, msg, nil, objc_classes, release, retain, Class, ClassExports, HostObject};
use crate::Environment;
use std::net::SocketAddrV4;

type SCNetworkReachabilityFlags = u32;
const kSCNetworkReachabilityFlagsReachable: SCNetworkReachabilityFlags = 1 << 1;
const kSCNetworkReachabilityFlagsIsDirect: SCNetworkReachabilityFlags = 1 << 17;

/// `SCNetworkReachabilityContext`
#[repr(C, packed)]
struct SCNetworkReachabilityContext {
    version: CFIndex,
    info: MutVoidPtr,
    retain_callback: GuestFunction,
    release_callback: GuestFunction,
    copy_desc_callback: GuestFunction,
}
unsafe impl SafeRead for SCNetworkReachabilityContext {}

#[derive(Default)]
pub struct State {
    /// Targets that have been scheduled on a run loop and have not been told
    /// their initial state yet. Each entry holds a retain on the target.
    pending_initial_callbacks: Vec<id>,
}
impl State {
    fn get(env: &mut Environment) -> &mut Self {
        &mut env
            .framework_state
            .system_configuration
            .sc_network_reachability
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// SCNetworkReachabilityRef is not explicitly stated to be CFType-based type,
// but the result of "Create" or "Copy" functions here is expected to be
// released with CFRelease().
@implementation _touchHLE_SCNetworkReachability: NSObject
@end

};

struct SCNetworkReachabilityHostObject {
    address: Option<SocketAddrV4>,
    /// `SCNetworkReachabilityCallBack`, null if there is none.
    callout: GuestFunction,
    /// `info` from the context passed to `SCNetworkReachabilitySetCallback`.
    info: MutVoidPtr,
    scheduled: bool,
}
impl HostObject for SCNetworkReachabilityHostObject {}

// See comment for `_touchHLE_SCNetworkReachability` class
type SCNetworkReachabilityRef = CFTypeRef;

fn new_target(env: &mut Environment, address: Option<SocketAddrV4>) -> SCNetworkReachabilityRef {
    let isa = env
        .objc
        .get_known_class("_touchHLE_SCNetworkReachability", &mut env.mem);
    env.objc.alloc_object(
        isa,
        Box::new(SCNetworkReachabilityHostObject {
            address,
            callout: GuestFunction::null_ptr(),
            info: MutVoidPtr::null(),
            scheduled: false,
        }),
        &mut env.mem,
    )
}

/// Whether `target` is something created by this module.
fn is_target(env: &mut Environment, target: SCNetworkReachabilityRef) -> bool {
    if target == nil {
        return false;
    }
    let target_class: Class = msg![env; target class];
    target_class
        == env
            .objc
            .get_known_class("_touchHLE_SCNetworkReachability", &mut env.mem)
}

fn SCNetworkReachabilityCreateWithName(
    env: &mut Environment,
    allocator: CFAllocatorRef,
    name: ConstPtr<u8>,
) -> SCNetworkReachabilityRef {
    assert!(allocator == kCFAllocatorDefault || env.mem.read(allocator).is_system_default()); // unimplemented
    let res = new_target(env, None);
    log_dbg!(
        "SCNetworkReachabilityCreateWithName({:?}, {:?} {:?}) -> {:?}",
        allocator,
        name,
        env.mem.cstr_at_utf8(name),
        res
    );
    res
}

fn SCNetworkReachabilityCreateWithAddress(
    env: &mut Environment,
    allocator: CFAllocatorRef,
    address: ConstPtr<sockaddr>,
) -> SCNetworkReachabilityRef {
    assert!(allocator == kCFAllocatorDefault || env.mem.read(allocator).is_system_default()); // unimplemented
    let address_val = env.mem.read(address);
    let res = new_target(env, Some(address_val.to_sockaddr_v4()));
    log_dbg!(
        "SCNetworkReachabilityCreateWithAddress({:?}, {:?} ({})) -> {:?}",
        allocator,
        address,
        address_val.to_sockaddr_v4(),
        res
    );
    res
}

/// The flags that describe `target`'s current state, or [None] if they can't
/// be determined (which is the case if the app isn't allowed network access).
fn current_flags(
    env: &mut Environment,
    target: SCNetworkReachabilityRef,
) -> Option<SCNetworkReachabilityFlags> {
    if !env.options.network_access {
        return None;
    }

    let host_object = env.objc.borrow::<SCNetworkReachabilityHostObject>(target);
    if let Some(addr) = host_object.address {
        if addr.ip().is_link_local() {
            // Those corresponds to local WiFi connection on a real iOS device
            return Some(
                kSCNetworkReachabilityFlagsReachable | kSCNetworkReachabilityFlagsIsDirect,
            );
        }
    }
    // TODO: actually check for the connectivity. For now the host is assumed
    // to be online, and reachable without needing to bring up a connection.
    Some(kSCNetworkReachabilityFlagsReachable)
}

fn SCNetworkReachabilityGetFlags(
    env: &mut Environment,
    target: SCNetworkReachabilityRef,
    flags: MutPtr<SCNetworkReachabilityFlags>,
) -> bool {
    if !is_target(env, target) {
        return false;
    }
    let Some(out_flags) = current_flags(env, target) else {
        log_dbg!(
            "Network access is disabled, SCNetworkReachabilityGetFlags({:?}, {:?}) -> false",
            target,
            flags
        );
        return false;
    };
    log_dbg!(
        "SCNetworkReachabilityGetFlags({:?}, {:?}) -> true, {:#x}",
        target,
        flags,
        out_flags
    );
    env.mem.write(flags, out_flags);
    true
}

fn SCNetworkReachabilitySetCallback(
    env: &mut Environment,
    target: SCNetworkReachabilityRef,
    callout: GuestFunction, // SCNetworkReachabilityCallBack
    context: MutPtr<SCNetworkReachabilityContext>,
) -> bool {
    if !is_target(env, target) {
        return false;
    }

    let info = if context.is_null() {
        MutVoidPtr::null()
    } else {
        let context = env.mem.read(context);
        // TODO: handle non-NULL retain and release callbacks
        let retain_callback = context.retain_callback;
        let release_callback = context.release_callback;
        assert!(retain_callback.to_ptr().is_null());
        assert!(release_callback.to_ptr().is_null());
        context.info
    };

    log_dbg!(
        "SCNetworkReachabilitySetCallback({:?}, {:?}, info {:?}) -> true",
        target,
        callout,
        info
    );
    let host_object = env
        .objc
        .borrow_mut::<SCNetworkReachabilityHostObject>(target);
    host_object.callout = callout;
    host_object.info = info;
    true
}

fn SCNetworkReachabilityScheduleWithRunLoop(
    env: &mut Environment,
    target: SCNetworkReachabilityRef,
    _run_loop: CFRunLoopRef,
    _run_loop_mode: CFRunLoopMode,
) -> bool {
    if !is_target(env, target) {
        return false;
    }

    let host_object = env
        .objc
        .borrow_mut::<SCNetworkReachabilityHostObject>(target);
    // Like the real thing, scheduling needs a callback to deliver to.
    if host_object.callout.to_ptr().is_null() {
        log!(
            "Warning: SCNetworkReachabilityScheduleWithRunLoop({:?}) without a callback -> false",
            target
        );
        return false;
    }
    if host_object.scheduled {
        return true;
    }
    host_object.scheduled = true;

    // The initial state is delivered the next time the run loop comes around.
    retain(env, target);
    State::get(env).pending_initial_callbacks.push(target);
    log_dbg!(
        "SCNetworkReachabilityScheduleWithRunLoop({:?}) -> true",
        target
    );
    true
}

fn SCNetworkReachabilityUnscheduleFromRunLoop(
    env: &mut Environment,
    target: SCNetworkReachabilityRef,
    _run_loop: CFRunLoopRef,
    _run_loop_mode: CFRunLoopMode,
) -> bool {
    if !is_target(env, target) {
        return false;
    }

    let host_object = env
        .objc
        .borrow_mut::<SCNetworkReachabilityHostObject>(target);
    if !std::mem::take(&mut host_object.scheduled) {
        return false;
    }

    // If the initial state hasn't been delivered yet, it now never will be.
    let pending = &mut State::get(env).pending_initial_callbacks;
    let mut removed = 0;
    pending.retain(|&pending_target| {
        let keep = pending_target != target;
        if !keep {
            removed += 1;
        }
        keep
    });
    for _ in 0..removed {
        release(env, target);
    }
    log_dbg!(
        "SCNetworkReachabilityUnscheduleFromRunLoop({:?}) -> true",
        target
    );
    true
}

/// For use by `NSRunLoop` (via [super::handle_run_loop]): deliver the initial
/// state to targets that were scheduled since the last time this was called.
pub(super) fn handle_pending_callbacks(env: &mut Environment) {
    let pending = std::mem::take(&mut State::get(env).pending_initial_callbacks);
    for target in pending {
        let &SCNetworkReachabilityHostObject {
            callout,
            info,
            scheduled,
            ..
        } = env.objc.borrow(target);
        if scheduled && !callout.to_ptr().is_null() {
            let flags = current_flags(env, target).unwrap_or(0);
            log_dbg!(
                "Calling reachability callback {:?} for {:?} with flags {:#x}",
                callout,
                target,
                flags
            );
            () = callout.call_from_host(env, (target, flags, info));
        }
        // Balances the retain done when the target was scheduled.
        release(env, target);
    }
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(SCNetworkReachabilityCreateWithName(_, _)),
    export_c_func!(SCNetworkReachabilityCreateWithAddress(_, _)),
    export_c_func!(SCNetworkReachabilityGetFlags(_, _)),
    export_c_func!(SCNetworkReachabilitySetCallback(_, _, _)),
    export_c_func!(SCNetworkReachabilityScheduleWithRunLoop(_, _, _)),
    export_c_func!(SCNetworkReachabilityUnscheduleFromRunLoop(_, _, _)),
];
