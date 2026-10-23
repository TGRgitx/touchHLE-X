/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UITableViewController`.
//!
//! A view controller whose view is a `UITableView`, with itself as the data
//! source and delegate. Subclasses are expected to implement the data source
//! methods.

use crate::frameworks::core_graphics::CGRect;
use crate::frameworks::foundation::NSInteger;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release,
    ClassExports, NSZonePtr,
};

struct UITableViewControllerHostObject {
    superclass: super::UIViewControllerHostObject,
    /// `UITableViewStyle`
    style: NSInteger,
    clears_selection_on_view_will_appear: bool,
}
impl_HostObject_with_superclass!(UITableViewControllerHostObject);
impl Default for UITableViewControllerHostObject {
    fn default() -> Self {
        UITableViewControllerHostObject {
            superclass: Default::default(),
            style: 0,
            clears_selection_on_view_will_appear: true,
        }
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UITableViewController: UIViewController

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UITableViewControllerHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithStyle:(NSInteger)style { // UITableViewStyle
    let this: id = msg![env; this initWithNibName:nil bundle:nil];
    env.objc.borrow_mut::<UITableViewControllerHostObject>(this).style = style;
    this
}

- (())loadView {
    let style = env.objc.borrow::<UITableViewControllerHostObject>(this).style;
    let screen: id = msg_class![env; UIScreen mainScreen];
    let frame: CGRect = msg![env; screen applicationFrame];
    let table: id = msg_class![env; UITableView alloc];
    let table: id = msg![env; table initWithFrame:frame style:style];
    () = msg![env; table setDataSource:this];
    () = msg![env; table setDelegate:this];
    () = msg![env; this setView:table];
    // setView: retained it
    release(env, table);
}

- (id)tableView {
    msg![env; this view]
}
- (())setTableView:(id)table { // UITableView*
    () = msg![env; this setView:table];
}

- (bool)clearsSelectionOnViewWillAppear {
    env.objc
        .borrow::<UITableViewControllerHostObject>(this)
        .clears_selection_on_view_will_appear
}
- (())setClearsSelectionOnViewWillAppear:(bool)clears {
    env.objc
        .borrow_mut::<UITableViewControllerHostObject>(this)
        .clears_selection_on_view_will_appear = clears;
}

- (())viewWillAppear:(bool)animated {
    () = msg_super![env; this viewWillAppear:animated];
    let clears = env
        .objc
        .borrow::<UITableViewControllerHostObject>(this)
        .clears_selection_on_view_will_appear;
    if clears {
        let table: id = msg![env; this tableView];
        let path: id = msg![env; table indexPathForSelectedRow];
        if path != nil {
            () = msg![env; table deselectRowAtIndexPath:path animated:animated];
        }
    }
}

@end

};
