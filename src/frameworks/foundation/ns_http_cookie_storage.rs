/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSHTTPCookieStorage`.
//!
//! This is a minimal in-memory stub: cookies are kept in an array so that apps
//! can add, remove and list them, but they are not persisted and no networking
//! code consults them yet.

use super::NSUInteger;
use crate::objc::{
    autorelease, id, msg, msg_class, objc_classes, ClassExports, HostObject, NSZonePtr,
};

type NSHTTPCookieAcceptPolicy = NSUInteger;
const NSHTTPCookieAcceptPolicyAlways: NSHTTPCookieAcceptPolicy = 0;

#[derive(Default)]
pub struct State {
    /// `NSHTTPCookieStorage*`
    shared_storage: Option<id>,
}

struct NSHTTPCookieStorageHostObject {
    /// `NSMutableArray*` of `NSHTTPCookie*`
    cookies: id,
    accept_policy: NSHTTPCookieAcceptPolicy,
}
impl HostObject for NSHTTPCookieStorageHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSHTTPCookieStorage: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let cookies: id = msg_class![env; NSMutableArray new];
    let host_object = Box::new(NSHTTPCookieStorageHostObject {
        cookies,
        accept_policy: NSHTTPCookieAcceptPolicyAlways,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)sharedHTTPCookieStorage {
    if let Some(existing) = env.framework_state.foundation.ns_http_cookie_storage.shared_storage {
        existing
    } else {
        let new: id = msg![env; this alloc];
        let new: id = msg![env; new init];
        env.framework_state.foundation.ns_http_cookie_storage.shared_storage = Some(new);
        new
    }
}

- (id)cookies {
    let cookies = env.objc.borrow::<NSHTTPCookieStorageHostObject>(this).cookies;
    let copy: id = msg_class![env; NSArray arrayWithArray:cookies];
    autorelease(env, copy)
}

- (id)cookiesForURL:(id)_url { // NSURL*
    // TODO: match cookies against the URL's domain and path
    msg_class![env; NSArray array]
}

- (())setCookie:(id)cookie { // NSHTTPCookie*
    let cookies = env.objc.borrow::<NSHTTPCookieStorageHostObject>(this).cookies;
    // TODO: replace an existing cookie with the same name, domain and path
    let () = msg![env; cookies addObject:cookie];
}

- (())setCookies:(id)new_cookies forURL:(id)_url mainDocumentURL:(id)_main_url {
    // NSArray* of NSHTTPCookie*, NSURL*, NSURL*
    let cookies = env.objc.borrow::<NSHTTPCookieStorageHostObject>(this).cookies;
    let () = msg![env; cookies addObjectsFromArray:new_cookies];
}

- (())deleteCookie:(id)cookie { // NSHTTPCookie*
    let cookies = env.objc.borrow::<NSHTTPCookieStorageHostObject>(this).cookies;
    let () = msg![env; cookies removeObject:cookie];
}

- (NSHTTPCookieAcceptPolicy)cookieAcceptPolicy {
    env.objc.borrow::<NSHTTPCookieStorageHostObject>(this).accept_policy
}
- (())setCookieAcceptPolicy:(NSHTTPCookieAcceptPolicy)policy {
    env.objc.borrow_mut::<NSHTTPCookieStorageHostObject>(this).accept_policy = policy;
}

@end

};
