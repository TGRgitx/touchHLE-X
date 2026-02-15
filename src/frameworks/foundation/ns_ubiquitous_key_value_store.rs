//! Stub for NSUbiquitousKeyValueStore (iCloud key-value storage) for touchHLE
//! Used to prevent the game from crashing when calling defaultStore and working with settings

use crate::objc::{ObjC, Class, id, nil, impl_HostObject_with_superclass, HostObject};
use crate::objc::objects::TrivialHostObject;
use crate::mem::Mem;
use std::collections::HashMap;

// --------------------
// 1. HostObject
// --------------------
pub struct NSUbiquitousKeyValueStoreHostObject {
    pub superclass: TrivialHostObject,
    pub map: HashMap<String, id>,
}

impl_HostObject_with_superclass!(NSUbiquitousKeyValueStoreHostObject);

// --------------------
// 2. Singleton defaultStore
// --------------------
static mut DEFAULT_STORE: Option<id> = None;

pub fn default_store(objc: &mut ObjC, this: Class, mem: &mut Mem) -> id {
    unsafe {
        if let Some(obj) = DEFAULT_STORE {
            return obj;
        }

        let host = NSUbiquitousKeyValueStoreHostObject {
            superclass: TrivialHostObject,
            map: HashMap::new(),
        };

        let obj = objc.alloc_object(this, Box::new(host), mem);
        DEFAULT_STORE = Some(obj);
        obj
    }
}

// --------------------
// 3. setObject:forKey:
// --------------------
pub fn set_object_for_key(
    objc: &mut ObjC,
    this: id,
    value: id,
    key: id,
    mem: &mut Mem,
) {
    let host = objc.borrow_mut::<NSUbiquitousKeyValueStoreHostObject>(this);

    let key_str = crate::frameworks::foundation::ns_string::to_rust_string(key, mem);

    host.map.insert(key_str, value);
}

// --------------------
// 4. objectForKey:
// --------------------
pub fn object_for_key(
    objc: &mut ObjC,
    this: id,
    key: id,
    mem: &mut Mem,
) -> id {
    let host = objc.borrow::<NSUbiquitousKeyValueStoreHostObject>(this);

    let key_str = crate::frameworks::foundation::ns_string::to_rust_string(key, mem);

    host.map.get(&key_str).copied().unwrap_or(nil)
}

// --------------------
// 5. removeObjectForKey:
// --------------------
pub fn remove_object_for_key(
    objc: &mut ObjC,
    this: id,
    key: id,
    mem: &mut Mem,
) {
    let host = objc.borrow_mut::<NSUbiquitousKeyValueStoreHostObject>(this);

    let key_str = crate::frameworks::foundation::ns_string::to_rust_string(key, mem);

    host.map.remove(&key_str);
}

// --------------------
// 6. synchronize
// --------------------
pub fn synchronize(_: &mut ObjC, _: id, _: &mut Mem) -> bool {
    true
}

// --------------------
// 7. Registering a class and methods
// --------------------
pub fn register_ns_ubiquitous_key_value_store(objc: &mut ObjC, superclass: Class, mem: &mut Mem) -> Class {
    let class = objc.register_class("NSUbiquitousKeyValueStore", superclass);

    class.add_class_method("defaultStore", default_store);
    class.add_method("setObject:forKey:", set_object_for_key);
    class.add_method("objectForKey:", object_for_key);
    class.add_method("removeObjectForKey:", remove_object_for_key);
    class.add_method("synchronize", synchronize);

    class
}