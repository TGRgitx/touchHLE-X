/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSIndexPath`.
//!
//! Immutable path of indices. The UIKit additions (`row` and `section`) used
//! by `UITableView` are included.

use super::ns_string::from_rust_ordering;
use super::{NSComparisonResult, NSInteger, NSNotFound, NSUInteger};
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, retain, Class, ClassExports, HostObject,
    NSZonePtr,
};
use crate::Environment;

struct NSIndexPathHostObject {
    indexes: Vec<NSUInteger>,
}
impl HostObject for NSIndexPathHostObject {}

/// Make a new autoreleased `NSIndexPath` with the given indices.
fn make_path(env: &mut Environment, class: Class, indexes: Vec<NSUInteger>) -> id {
    let new: id = msg![env; class alloc];
    env.objc.borrow_mut::<NSIndexPathHostObject>(new).indexes = indexes;
    autorelease(env, new)
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSIndexPath: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSIndexPathHostObject { indexes: Vec::new() });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)indexPathWithIndex:(NSUInteger)index {
    make_path(env, this, vec![index])
}

+ (id)indexPathForRow:(NSInteger)row inSection:(NSInteger)section {
    make_path(env, this, vec![section as NSUInteger, row as NSUInteger])
}

- (id)initWithIndex:(NSUInteger)index {
    env.objc.borrow_mut::<NSIndexPathHostObject>(this).indexes = vec![index];
    this
}

- (id)indexPathByAddingIndex:(NSUInteger)index {
    let mut indexes = env.objc.borrow::<NSIndexPathHostObject>(this).indexes.clone();
    indexes.push(index);
    let class: Class = msg![env; this class];
    make_path(env, class, indexes)
}

- (NSUInteger)length {
    env.objc.borrow::<NSIndexPathHostObject>(this).indexes.len() as NSUInteger
}

- (NSUInteger)indexAtPosition:(NSUInteger)position {
    let indexes = &env.objc.borrow::<NSIndexPathHostObject>(this).indexes;
    indexes.get(position as usize).copied().unwrap_or(NSNotFound as NSUInteger)
}

// UIKit additions
- (NSInteger)section {
    let indexes = &env.objc.borrow::<NSIndexPathHostObject>(this).indexes;
    indexes.first().copied().unwrap_or(0) as NSInteger
}
- (NSInteger)row {
    let indexes = &env.objc.borrow::<NSIndexPathHostObject>(this).indexes;
    indexes.get(1).copied().unwrap_or(0) as NSInteger
}

- (NSComparisonResult)compare:(id)other { // NSIndexPath *
    let a = env.objc.borrow::<NSIndexPathHostObject>(this).indexes.clone();
    let b = env.objc.borrow::<NSIndexPathHostObject>(other).indexes.clone();
    from_rust_ordering(a.cmp(&b))
}

- (bool)isEqual:(id)other {
    if this == other {
        return true;
    }
    if other == nil {
        return false;
    }
    let class: Class = msg_class![env; NSIndexPath class];
    if !msg![env; other isKindOfClass:class] {
        return false;
    }
    let a = &env.objc.borrow::<NSIndexPathHostObject>(this).indexes;
    let b = &env.objc.borrow::<NSIndexPathHostObject>(other).indexes;
    a == b
}

- (NSUInteger)hash {
    let indexes = env.objc.borrow::<NSIndexPathHostObject>(this).indexes.clone();
    super::hash_helper(&indexes)
}

// NSCopying implementation (immutable, so just retain)
- (id)copyWithZone:(NSZonePtr)_zone {
    retain(env, this)
}

@end

};
