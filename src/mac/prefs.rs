//! NSUserDefaults access. Keys are the same as the original OpenKey app, whose settings and
//! macros are copied over on first run.

use objc2::rc::Retained;
use objc2_foundation::{NSBundle, NSData, NSString, NSUserDefaults};

fn defaults() -> Retained<NSUserDefaults> {
    NSUserDefaults::standardUserDefaults()
}

/// Integer value, or `default` when the key was never written.
/// First run (no settings of our own yet): copy the settings of the app with bundle id `from`.
pub fn import_domain(from: &str) {
    let d = defaults();
    let Some(me) = NSBundle::mainBundle().bundleIdentifier() else { return };
    if d.persistentDomainForName(&me).is_some() {
        return;
    }
    if let Some(old) = d.persistentDomainForName(&NSString::from_str(from)) {
        unsafe { d.setPersistentDomain_forName(&old, &me) };
    }
}

pub fn int(key: &str, default: i64) -> i64 {
    let k = NSString::from_str(key);
    let d = defaults();
    if d.objectForKey(&k).is_none() {
        return default;
    }
    d.integerForKey(&k) as i64
}

pub fn flag(key: &str, default: bool) -> bool {
    int(key, default as i64) != 0
}

pub fn set_int(key: &str, v: i64) {
    defaults().setInteger_forKey(v as isize, &NSString::from_str(key));
}

pub fn data(key: &str) -> Vec<u8> {
    defaults().dataForKey(&NSString::from_str(key)).map(|d| d.to_vec()).unwrap_or_default()
}

pub fn set_data(key: &str, bytes: &[u8]) {
    let d = NSData::with_bytes(bytes);
    unsafe { defaults().setObject_forKey(Some(&d), &NSString::from_str(key)) };
}
