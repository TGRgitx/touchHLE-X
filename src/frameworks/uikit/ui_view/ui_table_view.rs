/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UITableView`.
//!
//! This is only a placeholder: it is a scroll view that remembers its data
//! source and delegate but never queries them or draws any rows. It exists so
//! that apps which merely create table views (e.g. the Crystal social SDK
//! bundled with some games) don't crash.

use crate::frameworks::core_graphics::CGRect;
use crate::frameworks::foundation::NSInteger;
use crate::objc::{id, msg, nil, objc_classes, ClassExports};

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UITableView: UIScrollView

- (id)initWithFrame:(CGRect)frame
              style:(NSInteger)_style {
    msg![env; this initWithFrame:frame]
}

- (id)dataSource {
    nil
}
- (())setDataSource:(id)_data_source {
    // TODO
}

- (())reloadData {
    // TODO
}

- (())setSeparatorStyle:(NSInteger)_style {
    // TODO
}
- (())setRowHeight:(f32)_height {
    // TODO
}
- (())setAllowsSelection:(bool)_allows {
    // TODO
}
- (())setSectionHeaderHeight:(f32)_height {
    // TODO
}
- (())setSectionFooterHeight:(f32)_height {
    // TODO
}
- (())setTableHeaderView:(id)_view {
    // TODO
}
- (())setTableFooterView:(id)_view {
    // TODO
}
- (())setBackgroundView:(id)_view {
    // TODO
}

@end

};
