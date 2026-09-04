//! Exact impl of NSDateComponents.
//! Conforming to iOS 3.0+ Foundation specifications.

use crate::mem::Mem;
use crate::objc::{id, impl_HostObject_with_superclass, nil, Class, ObjC, TrivialHostObject};

/// Apple uses NSIntegerMax (0x7fffffff on 32-bit) for undefined components
const NS_UNDEFINED: i32 = 0x7fffffff;

// --------------------
// 1. HostObject
// --------------------
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

// --------------------
// 2. alloc / init
// --------------------
fn alloc(objc: &mut ObjC, this: Class, mem: &mut Mem) -> id {
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

fn init(_objc: &mut ObjC, this: id, _mem: &mut Mem) -> id {
    this
}

// --------------------
// 3. Getters & Setters for Objects
// --------------------
fn calendar(objc: &mut ObjC, this: id, _mem: &mut Mem) -> id {
    objc.borrow::<NSDateComponentsHostObject>(this).calendar
}

fn set_calendar(objc: &mut ObjC, this: id, val: id, _mem: &mut Mem) {
    objc.borrow_mut::<NSDateComponentsHostObject>(this).calendar = val;
}

fn time_zone(objc: &mut ObjC, this: id, _mem: &mut Mem) -> id {
    objc.borrow::<NSDateComponentsHostObject>(this).time_zone
}

fn set_time_zone(objc: &mut ObjC, this: id, val: id, _mem: &mut Mem) {
    objc.borrow_mut::<NSDateComponentsHostObject>(this)
        .time_zone = val;
}

// --------------------
// 4. Macro for Integer Properties
// --------------------
macro_rules! make_integer_accessor {
    ($get_name:ident, $set_name:ident, $field:ident) => {
        fn $get_name(objc: &mut ObjC, this: id, _mem: &mut Mem) -> i32 {
            objc.borrow::<NSDateComponentsHostObject>(this).$field
        }
        fn $set_name(objc: &mut ObjC, this: id, val: i32, _mem: &mut Mem) {
            objc.borrow_mut::<NSDateComponentsHostObject>(this).$field = val;
        }
    };
}

make_integer_accessor!(era, set_era, era);
make_integer_accessor!(year, set_year, year);
make_integer_accessor!(month, set_month, month);
make_integer_accessor!(day, set_day, day);
make_integer_accessor!(hour, set_hour, hour);
make_integer_accessor!(minute, set_minute, minute);
make_integer_accessor!(second, set_second, second);
make_integer_accessor!(week, set_week, week);
make_integer_accessor!(weekday, set_weekday, weekday);
make_integer_accessor!(weekday_ordinal, set_weekday_ordinal, weekday_ordinal);

// --------------------
// 5. Registering the class and its methods
// --------------------
pub fn register_ns_date_components(objc: &mut ObjC, superclass: Class, mem: &mut Mem) -> Class {
    let class = objc.register_class("NSDateComponents", superclass);

    class.add_class_method("alloc", alloc);
    class.add_method("init", init);

    class.add_method("calendar", calendar);
    class.add_method("setCalendar:", set_calendar);
    class.add_method("timeZone", time_zone);
    class.add_method("setTimeZone:", set_time_zone);

    class.add_method("era", era);
    class.add_method("setEra:", set_era);
    class.add_method("year", year);
    class.add_method("setYear:", set_year);
    class.add_method("month", month);
    class.add_method("setMonth:", set_month);
    class.add_method("day", day);
    class.add_method("setDay:", set_day);
    class.add_method("hour", hour);
    class.add_method("setHour:", set_hour);
    class.add_method("minute", minute);
    class.add_method("setMinute:", set_minute);
    class.add_method("second", second);
    class.add_method("setSecond:", set_second);
    class.add_method("week", week);
    class.add_method("setWeek:", set_week);
    class.add_method("weekday", weekday);
    class.add_method("setWeekday:", set_weekday);
    class.add_method("weekdayOrdinal", weekday_ordinal);
    class.add_method("setWeekdayOrdinal:", set_weekday_ordinal);

    class
}
