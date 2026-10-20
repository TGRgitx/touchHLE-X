/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSDateComponents`.
//!
//! This is just a container for values: it does not do any calendar
//! calculations itself.

use super::NSInteger;
use crate::objc::{
    id, msg, nil, objc_classes, release, retain, Class, ClassExports, HostObject, NSZonePtr,
};

/// `NSUndefinedDateComponent`, i.e. `NSIntegerMax` (0x7fffffff on 32-bit).
pub const NS_UNDEFINED_DATE_COMPONENT: NSInteger = 0x7fffffff;

struct NSDateComponentsHostObject {
    /// `NSCalendar*`
    calendar: id,
    /// `NSTimeZone*`
    time_zone: id,
    era: NSInteger,
    year: NSInteger,
    month: NSInteger,
    day: NSInteger,
    hour: NSInteger,
    minute: NSInteger,
    second: NSInteger,
    week: NSInteger,
    weekday: NSInteger,
    weekday_ordinal: NSInteger,
}
impl HostObject for NSDateComponentsHostObject {}

impl Default for NSDateComponentsHostObject {
    fn default() -> Self {
        NSDateComponentsHostObject {
            calendar: nil,
            time_zone: nil,
            era: NS_UNDEFINED_DATE_COMPONENT,
            year: NS_UNDEFINED_DATE_COMPONENT,
            month: NS_UNDEFINED_DATE_COMPONENT,
            day: NS_UNDEFINED_DATE_COMPONENT,
            hour: NS_UNDEFINED_DATE_COMPONENT,
            minute: NS_UNDEFINED_DATE_COMPONENT,
            second: NS_UNDEFINED_DATE_COMPONENT,
            week: NS_UNDEFINED_DATE_COMPONENT,
            weekday: NS_UNDEFINED_DATE_COMPONENT,
            weekday_ordinal: NS_UNDEFINED_DATE_COMPONENT,
        }
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSDateComponents: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<NSDateComponentsHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)init {
    this
}

- (())dealloc {
    let &NSDateComponentsHostObject { calendar, time_zone, .. } = env.objc.borrow(this);
    release(env, calendar);
    release(env, time_zone);
    env.objc.dealloc_object(this, &mut env.mem)
}

// NSCopying implementation
- (id)copyWithZone:(NSZonePtr)_zone {
    let &NSDateComponentsHostObject {
        calendar, time_zone, era, year, month, day, hour, minute, second,
        week, weekday, weekday_ordinal
    } = env.objc.borrow(this);
    retain(env, calendar);
    retain(env, time_zone);
    let host_object = Box::new(NSDateComponentsHostObject {
        calendar, time_zone, era, year, month, day, hour, minute, second,
        week, weekday, weekday_ordinal
    });
    let class: Class = msg![env; this class];
    env.objc.alloc_object(class, host_object, &mut env.mem)
}

- (id)calendar {
    env.objc.borrow::<NSDateComponentsHostObject>(this).calendar
}
- (())setCalendar:(id)calendar { // NSCalendar *
    retain(env, calendar);
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<NSDateComponentsHostObject>(this).calendar,
        calendar,
    );
    release(env, old);
}

- (id)timeZone {
    env.objc.borrow::<NSDateComponentsHostObject>(this).time_zone
}
- (())setTimeZone:(id)time_zone { // NSTimeZone *
    retain(env, time_zone);
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<NSDateComponentsHostObject>(this).time_zone,
        time_zone,
    );
    release(env, old);
}

- (NSInteger)era {
    env.objc.borrow::<NSDateComponentsHostObject>(this).era
}
- (())setEra:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).era = value;
}

- (NSInteger)year {
    env.objc.borrow::<NSDateComponentsHostObject>(this).year
}
- (())setYear:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).year = value;
}

- (NSInteger)month {
    env.objc.borrow::<NSDateComponentsHostObject>(this).month
}
- (())setMonth:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).month = value;
}

- (NSInteger)day {
    env.objc.borrow::<NSDateComponentsHostObject>(this).day
}
- (())setDay:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).day = value;
}

- (NSInteger)hour {
    env.objc.borrow::<NSDateComponentsHostObject>(this).hour
}
- (())setHour:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).hour = value;
}

- (NSInteger)minute {
    env.objc.borrow::<NSDateComponentsHostObject>(this).minute
}
- (())setMinute:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).minute = value;
}

- (NSInteger)second {
    env.objc.borrow::<NSDateComponentsHostObject>(this).second
}
- (())setSecond:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).second = value;
}

- (NSInteger)week {
    env.objc.borrow::<NSDateComponentsHostObject>(this).week
}
- (())setWeek:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).week = value;
}

- (NSInteger)weekday {
    env.objc.borrow::<NSDateComponentsHostObject>(this).weekday
}
- (())setWeekday:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).weekday = value;
}

- (NSInteger)weekdayOrdinal {
    env.objc.borrow::<NSDateComponentsHostObject>(this).weekday_ordinal
}
- (())setWeekdayOrdinal:(NSInteger)value {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).weekday_ordinal = value;
}

@end

};
