//! Exact impl of NSDateComponents.
// Conforming to iOS 3.0+ Foundation specifications.

use crate::dyld::ClassExports;
use crate::mem::Mem;
use crate::objc::objects::TrivialHostObject;
use crate::objc::{
    id, impl_HostObject_with_superclass, nil, Class, HostObject, ObjC,
};

/// Apple uses NSIntegerMax (0x7fffffff on 32-bit) for undefined components
const NS_UNDEFINED: isize = 0x7fffffff;

pub struct NSDateComponentsHostObject {
    pub superclass: TrivialHostObject,
    pub calendar: id,
    pub time_zone: id,
    pub era: isize,
    pub year: isize,
    pub month: isize,
    pub day: isize,
    pub hour: isize,
    pub minute: isize,
    pub second: isize,
    pub week: isize,
    pub weekday: isize,
    pub weekday_ordinal: isize,
}

impl_HostObject_with_superclass!(NSDateComponentsHostObject);

pub fn alloc_components(objc: &mut ObjC, this: Class, mem: &mut Mem) -> id {
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
    objc.alloc_object(this, Box::new(host), mem)
}

pub fn init_components(_: &mut ObjC, this: id, _: &mut Mem) -> id {
    this
}

// --------------------
// Getters & Setters for Objects
// --------------------
pub fn get_calendar(objc: &mut ObjC, this: id, _: &mut Mem) -> id {
    objc.borrow::<NSDateComponentsHostObject>(this).calendar
}
pub fn set_calendar(objc: &mut ObjC, this: id, val: id, _: &mut Mem) {
    objc.borrow_mut::<NSDateComponentsHostObject>(this).calendar = val;
}
pub fn get_time_zone(objc: &mut ObjC, this: id, _: &mut Mem) -> id {
    objc.borrow::<NSDateComponentsHostObject>(this).time_zone
}
pub fn set_time_zone(objc: &mut ObjC, this: id, val: id, _: &mut Mem) {
    objc.borrow_mut::<NSDateComponentsHostObject>(this)
        .time_zone = val;
}

// --------------------
// Macro for Integer Properties to save space and keep < 80 chars
// --------------------
macro_rules! make_integer_accessor {
    ($get_name:ident, $set_name:ident, $field:ident) => {
        pub fn $get_name(objc: &mut ObjC, this: id, _: &mut Mem) -> isize {
            objc.borrow::<NSDateComponentsHostObject>(this).$field
        }
        pub fn $set_name(objc: &mut ObjC, this: id, val: isize, _: &mut Mem) {
            objc.borrow_mut::<NSDateComponentsHostObject>(this).$field = val;
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
make_integer_accessor!(
    get_weekday_ordinal,
    set_weekday_ordinal,
    weekday_ordinal
);

// --------------------
// Registration Export for touchHLE
// --------------------
fn register(objc: &mut ObjC, class: Class) -> Class {
    class.add_class_method("alloc", alloc_components);
    class.add_method("init", init_components);

    // Object properties
    class.add_method("calendar", get_calendar);
    class.add_method("setCalendar:", set_calendar);
    class.add_method("timeZone", get_time_zone);
    class.add_method("setTimeZone:", set_time_zone);

    // Integer properties
    class.add_method("era", get_era);
    class.add_method("setEra:", set_era);
    class.add_method("year", get_year);
    class.add_method("setYear:", set_year);
    class.add_method("month", get_month);
    class.add_method("setMonth:", set_month);
    class.add_method("day", get_day);
    class.add_method("setDay:", set_day);
    class.add_method("hour", get_hour);
    class.add_method("setHour:", set_hour);
    class.add_method("minute", get_minute);
    class.add_method("setMinute:", set_minute);
    class.add_method("second", get_second);
    class.add_method("setSecond:", set_second);
    class.add_method("week", get_week);
    class.add_method("setWeek:", set_week);
    class.add_method("weekday", get_weekday);
    class.add_method("setWeekday:", set_weekday);
    class.add_method("weekdayOrdinal", get_weekday_ordinal);
    class.add_method("setWeekdayOrdinal:", set_weekday_ordinal);

    class
}

pub const CLASSES: ClassExports = &[crate::dyld::ClassExport {
    name: "NSDateComponents",
    superclass_name: "NSObject",
    init_fn: Some(register),
}];
