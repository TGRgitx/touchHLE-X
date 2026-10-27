/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! SystemConfiguration framework.

mod sc_network_reachability;

#[derive(Default)]
pub struct State {
    sc_network_reachability: sc_network_reachability::State,
}

/// For use by `NSRunLoop`: deliver pending SystemConfiguration callbacks.
pub fn handle_run_loop(env: &mut crate::Environment) {
    sc_network_reachability::handle_pending_callbacks(env);
}

pub const DYLIB: crate::dyld::HostDylib = crate::dyld::HostDylib {
    path: "/System/Library/Frameworks/SystemConfiguration.framework/SystemConfiguration",
    aliases: &[],
    class_exports: &[sc_network_reachability::CLASSES],
    constant_exports: &[],
    function_exports: &[sc_network_reachability::FUNCTIONS],
};
