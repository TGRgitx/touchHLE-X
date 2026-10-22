/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UITableViewController`.
//!
//! This is only a placeholder so that apps which subclass it (e.g. the
//! Crystal social SDK bundled with some games) can at least load their
//! classes. The `UITableView` it creates is a stub too, so none of the table
//! functionality works.

use crate::frameworks::core_graphics::CGRect;
use crate::objc::{id, msg, msg_class, objc_classes, ClassExports};

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UITableViewController: UIViewController

- (())loadView {
    let screen: id = msg_class![env; UIScreen mainScreen];
    let frame: CGRect = msg![env; screen applicationFrame];
    let table: id = msg_class![env; UITableView alloc];
    let table: id = msg![env; table initWithFrame:frame style:0i32];
    () = msg![env; this setView:table];
    // setView: retained it
    crate::objc::release(env, table);
}

- (id)tableView {
    msg![env; this view]
}

// TODO: implement the table view controller on top of a real UITableView

@end

};
