//! Exact impl of NSDateComponents.
// Conforming to iOS 3.0+ Foundation specifications.

use crate::objc::{
    id, impl_HostObject_with_superclass, nil, objc_method, ClassExports, ClassTemplate, Sel,
    TrivialHostObject,
};
use crate::Environment;

/// Apple uses NSIntegerMax (0x7fffffff on 32-bit) for undefined components
const NS_UNDEFINED: i32 = 0x7fffffff;

pub struct NSDateComponentsHostObject {
    pub superclass: TrivialHostObject,
    pub calendar: id,
    pub time_zone: id,
    pub era: i32,
    pub year: i32,
    pub month: i32,
    pub day: i32,
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
    pub week: i32,
    pub weekday: i32,
    pub weekday_ordinal: i32,
}

impl_HostObject_with_superclass!(NSDateComponentsHostObject);

fn alloc(env: &mut Environment, this: id, _sel: Sel) -> id {
    let host = NSDateComponentsHostObject {
        superclass: TrivialHostObject,
        calendar: nil,
        time_zone: nil,
        era: NS_UNDEFINED,
        year: NS_UNDEFINED,
        month: NS_UNDEFINED,
        day: NS_UNDEFINED,
        hour: NS_UNDEFINED,
        minute: NS_UNDEFINED,
        second: NS_UNDEFINED,
        week: NS_UNDEFINED,
        weekday: NS_UNDEFINED,
        weekday_ordinal: NS_UNDEFINED,
    };
    env.objc.alloc_object(this, Box::new(host), &mut env.mem)
}

fn init(_env: &mut Environment, this: id, _sel: Sel) -> id {
    this
}

// --------------------
// Getters & Setters for Objects
// --------------------
fn get_calendar(env: &mut Environment, this: id, _sel: Sel) -> id {
    env.objc.borrow::<NSDateComponentsHostObject>(this).calendar
}

fn set_calendar(env: &mut Environment, this: id, _sel: Sel, val: id) {
    env.objc
        .borrow_mut::<NSDateComponentsHostObject>(this)
        .calendar = val;
}

fn get_time_zone(env: &mut Environment, this: id, _sel: Sel) -> id {
    env.objc
        .borrow::<NSDateComponentsHostObject>(this)
        .time_zone
}

fn set_time_zone(env: &mut Environment, this: id, _sel: Sel, val: id) {
    env.objc
        .borrow_mut::<NSDateComponentsHostObject>(this)
        .time_zone = val;
}

// --------------------
// Macro for Integer Properties
// --------------------
macro_rules! make_integer_accessor {
    ($get_name:ident, $set_name:ident, $field:ident) => {
        fn $get_name(env: &mut Environment, this: id, _sel: Sel) -> i32 {
            env.objc.borrow::<NSDateComponentsHostObject>(this).$field
        }
        fn $set_name(env: &mut Environment, this: id, _sel: Sel, val: i32) {
            env.objc
                .borrow_mut::<NSDateComponentsHostObject>(this)
                .$field = val;
        }
    };
}

make_integer_accessor!(get_era, set_era, era);
make_integer_accessor!(get_year, set_year, year);
make_integer_accessor!(get_month, set_month, month);
make_integer_accessor!(get_day, set_day, day);
make_integer_accessor!(get_hour, set_hour, hour);
make_integer_accessor!(get_minute, set_minute, minute);
make_integer_accessor!(get_second, set_second, second);
make_integer_accessor!(get_week, set_week, week);
make_integer_accessor!(get_weekday, set_weekday, weekday);
make_integer_accessor!(get_weekday_ordinal, set_weekday_ordinal, weekday_ordinal);

// --------------------
// Registration Export for touchHLE
// --------------------
pub const CLASSES: ClassExports = &[(
    "NSDateComponents",
    ClassTemplate {
        superclass_name: "NSObject",
        instance_methods: &[
            ("init", objc_method!(init)),
            ("calendar", objc_method!(get_calendar)),
            ("setCalendar:", objc_method!(set_calendar)),
            ("timeZone", objc_method!(get_time_zone)),
            ("setTimeZone:", objc_method!(set_time_zone)),
            ("era", objc_method!(get_era)),
            ("setEra:", objc_method!(set_era)),
            ("year", objc_method!(get_year)),
            ("setYear:", objc_method!(set_year)),
            ("month", objc_method!(get_month)),
            ("setMonth:", objc_method!(set_month)),
            ("day", objc_method!(get_day)),
            ("setDay:", objc_method!(set_day)),
            ("hour", objc_method!(get_hour)),
            ("setHour:", objc_method!(set_hour)),
            ("minute", objc_method!(get_minute)),
            ("setMinute:", objc_method!(set_minute)),
            ("second", objc_method!(get_second)),
            ("setSecond:", objc_method!(set_second)),
            ("week", objc_method!(get_week)),
            ("setWeek:", objc_method!(set_week)),
            ("weekday", objc_method!(get_weekday)),
            ("setWeekday:", objc_method!(set_weekday)),
            ("weekdayOrdinal", objc_method!(get_weekday_ordinal)),
            ("setWeekdayOrdinal:", objc_method!(set_weekday_ordinal)),
        ],
        class_methods: &[("alloc", objc_method!(alloc))],
    },
)];
