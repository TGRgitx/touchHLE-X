//! Exact impl of NSDateComponents.
//! Conforming to iOS 3.0+ Foundation specifications.

use crate::objc::{id, impl_HostObject_with_superclass, nil, ClassExports, TrivialHostObject, SEL};
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

fn alloc(env: &mut Environment, this: id, _sel: SEL) -> id {
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

fn init(_env: &mut Environment, this: id, _sel: SEL) -> id {
    this
}

// --------------------
// Getters & Setters for Objects
// --------------------
fn calendar(env: &mut Environment, this: id, _sel: SEL) -> id {
    env.objc.borrow::<NSDateComponentsHostObject>(this).calendar
}

fn setCalendar_(env: &mut Environment, this: id, _sel: SEL, val: id) {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).calendar = val;
}

fn timeZone(env: &mut Environment, this: id, _sel: SEL) -> id {
    env.objc.borrow::<NSDateComponentsHostObject>(this).time_zone
}

fn setTimeZone_(env: &mut Environment, this: id, _sel: SEL, val: id) {
    env.objc.borrow_mut::<NSDateComponentsHostObject>(this).time_zone = val;
}

// --------------------
// Macro for Integer Properties
// --------------------
macro_rules! make_integer_accessor {
    ($get_name:ident, $set_name:ident, $field:ident) => {
        fn $get_name(env: &mut Environment, this: id, _sel: SEL) -> i32 {
            env.objc.borrow::<NSDateComponentsHostObject>(this).$field
        }
        fn $set_name(env: &mut Environment, this: id, _sel: SEL, val: i32) {
            env.objc.borrow_mut::<NSDateComponentsHostObject>(this).$field = val;
        }
    };
}

make_integer_accessor!(era, setEra_, era);
make_integer_accessor!(year, setYear_, year);
make_integer_accessor!(month, setMonth_, month);
make_integer_accessor!(day, setDay_, day);
make_integer_accessor!(hour, setHour_, hour);
make_integer_accessor!(minute, setMinute_, minute);
make_integer_accessor!(second, setSecond_, second);
make_integer_accessor!(week, setWeek_, week);
make_integer_accessor!(weekday, setWeekday_, weekday);
make_integer_accessor!(weekdayOrdinal, setWeekdayOrdinal_, weekday_ordinal);

// --------------------
// Registration Export
// --------------------
pub const CLASSES: ClassExports = objc_classes! {
    class NSDateComponents: NSObject {
        + alloc
        - init
        - calendar
        - setCalendar_
        - timeZone
        - setTimeZone_
        - era
        - setEra_
        - year
        - setYear_
        - month
        - setMonth_
        - day
        - setDay_
        - hour
        - setHour_
        - minute
        - setMinute_
        - second
        - setSecond_
        - week
        - setWeek_
        - weekday
        - setWeekday_
        - weekdayOrdinal
        - setWeekdayOrdinal_
    }
};
