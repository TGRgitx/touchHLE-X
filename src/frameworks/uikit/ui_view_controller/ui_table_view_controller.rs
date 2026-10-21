/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UITableViewController`.
//!
//! This is only a placeholder so that apps which subclass it (e.g. the
//! Crystal social SDK bundled with some games) can at least load their
//! classes. There is no `UITableView` yet, so none of the table functionality
//! works.

use crate::objc::{objc_classes, ClassExports};

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UITableViewController: UIViewController

// TODO: implement UITableView and the table view controller on top of it

@end

};
