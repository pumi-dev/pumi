//! macOS: libproc and Mach for the processes and threads, the I/O Registry for the GPU.
use super::{Task, Thread};
use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetTypeID, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation_sys::base::{CFGetTypeID, CFRelease, CFTypeRef, kCFAllocatorDefault};
use core_foundation_sys::dictionary::{CFDictionaryGetTypeID, CFDictionaryGetValue, CFDictionaryRef};
use core_foundation_sys::number::{CFNumberGetTypeID, CFNumberGetValue, CFNumberRef, kCFNumberSInt64Type};
use core_foundation_sys::string::{
    CFStringCreateWithCString, CFStringGetCString, CFStringGetTypeID, CFStringRef, kCFStringEncodingUTF8,
};
use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::time::Duration;

/// The process that composites the screen.
pub const COMPOSITOR: &str = "WindowServer";

const PROC_NAME: usize = 256;
const MAX_PIDS: usize = 8192;

unsafe extern "C" {
    fn proc_listchildpids(ppid: c_int, buffer: *mut c_void, buffersize: c_int) -> c_int;
    fn proc_listallpids(buffer: *mut c_void, buffersize: c_int) -> c_int;
    fn mach_timebase_info(info: *mut [u32; 2]) -> c_int;
    static mach_task_self_: libc::mach_port_t;
    fn mach_port_deallocate(task: libc::mach_port_t, name: libc::mach_port_t) -> libc::kern_return_t;
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> *mut c_void;
    fn IOServiceGetMatchingServices(main: libc::mach_port_t, matching: *mut c_void, it: *mut u32) -> c_int;
    fn IOIteratorNext(it: u32) -> u32;
    fn IOObjectRelease(object: u32) -> c_int;
    fn IORegistryEntryGetChildIterator(entry: u32, plane: *const c_char, it: *mut u32) -> c_int;
    fn IORegistryEntryCreateCFProperty(
        entry: u32,
        key: CFStringRef,
        allocator: *const c_void,
        options: u32,
    ) -> CFTypeRef;
}

/// A process's name: libproc's, or (for another user's process, whose full name it refuses)
/// the short info's, which is at most 16 characters.
pub fn name(pid: i32) -> Option<String> {
    let mut buffer = [0u8; PROC_NAME];
    let len = unsafe { libc::proc_name(pid, buffer.as_mut_ptr().cast(), PROC_NAME as u32) };
    if len > 0 {
        return Some(String::from_utf8_lossy(&buffer[..len as usize]).into_owned());
    }
    let mut info: libc::proc_bsdshortinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdshortinfo>() as c_int;
    let read = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDT_SHORTBSDINFO, 0, (&raw mut info).cast(), size) };
    if read != size {
        return None;
    }
    let comm: Vec<u8> = info.pbsi_comm.iter().map(|c| *c as u8).take_while(|c| *c != 0).collect();
    (!comm.is_empty()).then(|| String::from_utf8_lossy(&comm).into_owned())
}

fn pids(list: impl FnOnce(*mut c_void, c_int) -> c_int) -> Vec<i32> {
    let mut buffer = vec![0i32; MAX_PIDS];
    let bytes = (buffer.len() * std::mem::size_of::<i32>()) as c_int;
    let count = list(buffer.as_mut_ptr().cast(), bytes).max(0) as usize;
    buffer.truncate(count.min(MAX_PIDS));
    buffer.retain(|pid| *pid > 0);
    buffer
}

pub fn children(pid: i32) -> Vec<i32> {
    pids(|buffer, size| unsafe { proc_listchildpids(pid, buffer, size) })
}

pub fn find(wanted: &str) -> Option<i32> {
    pids(|buffer, size| unsafe { proc_listallpids(buffer, size) })
        .into_iter()
        .find(|pid| name(*pid).as_deref() == Some(wanted))
}

/// Nanoseconds a Mach absolute-time tick is.
fn timebase() -> f64 {
    static TIMEBASE: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *TIMEBASE.get_or_init(|| {
        let mut info = [0u32; 2];
        match unsafe { mach_timebase_info(&mut info) } {
            0 if info[1] != 0 => f64::from(info[0]) / f64::from(info[1]),
            _ => 1.0,
        }
    })
}

pub fn task(pid: i32) -> Option<Task> {
    let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_taskinfo>() as c_int;
    let read = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTASKINFO, 0, (&raw mut info).cast(), size) };
    if read != size {
        return None;
    }
    let ticks = info.pti_total_user + info.pti_total_system;
    Some(Task {
        cpu: Duration::from_nanos((ticks as f64 * timebase()) as u64),
        rss: info.pti_resident_size,
        threads: info.pti_threadnum.max(0) as u32,
    })
}

pub fn threads() -> Vec<Thread> {
    unsafe {
        let task = mach_task_self_;
        let mut list: libc::thread_act_array_t = std::ptr::null_mut();
        let mut count: libc::mach_msg_type_number_t = 0;
        if libc::task_threads(task, &mut list, &mut count) != libc::KERN_SUCCESS || list.is_null() {
            return Vec::new();
        }
        let ports = std::slice::from_raw_parts(list, count as usize);
        let mut out = Vec::with_capacity(ports.len());
        for (n, &port) in ports.iter().enumerate() {
            let mut info: libc::thread_extended_info = std::mem::zeroed();
            let mut size = libc::THREAD_EXTENDED_INFO_COUNT;
            let extended = libc::thread_info(
                port,
                libc::THREAD_EXTENDED_INFO as libc::thread_flavor_t,
                (&raw mut info).cast(),
                &mut size,
            );
            let mut ident: libc::thread_identifier_info = std::mem::zeroed();
            let mut size = libc::THREAD_IDENTIFIER_INFO_COUNT;
            let identified = libc::thread_info(
                port,
                libc::THREAD_IDENTIFIER_INFO as libc::thread_flavor_t,
                (&raw mut ident).cast(),
                &mut size,
            );
            mach_port_deallocate(task, port);
            if extended != libc::KERN_SUCCESS || identified != libc::KERN_SUCCESS {
                continue;
            }
            let name = CStr::from_ptr(info.pth_name.as_ptr()).to_string_lossy().into_owned();
            let name = match name.is_empty() {
                true if n == 0 => "main".to_string(),
                true => "unnamed".to_string(),
                false => name,
            };
            let cpu = Duration::from_nanos(info.pth_user_time + info.pth_system_time);
            out.push(Thread { id: ident.thread_id, name, cpu });
        }
        libc::vm_deallocate(
            task,
            list as libc::vm_address_t,
            count as libc::vm_size_t * std::mem::size_of::<libc::thread_act_t>() as libc::vm_size_t,
        );
        out
    }
}

/// A CF property of an I/O Registry entry, owned.
struct Owned(CFTypeRef);
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

fn cf_string(text: &str) -> Option<Owned> {
    let text = std::ffi::CString::new(text).ok()?;
    let made = unsafe { CFStringCreateWithCString(kCFAllocatorDefault, text.as_ptr(), kCFStringEncodingUTF8) };
    (!made.is_null()).then(|| Owned(made.cast()))
}

fn property(entry: u32, key: &str) -> Option<Owned> {
    let key = cf_string(key)?;
    let value = unsafe { IORegistryEntryCreateCFProperty(entry, key.0.cast(), kCFAllocatorDefault.cast(), 0) };
    (!value.is_null()).then(|| Owned(value))
}

fn is(value: CFTypeRef, type_id: usize) -> bool {
    !value.is_null() && unsafe { CFGetTypeID(value) } == type_id
}

fn get(dictionary: CFTypeRef, key: &str) -> CFTypeRef {
    if !is(dictionary, unsafe { CFDictionaryGetTypeID() }) {
        return std::ptr::null();
    }
    let Some(key) = cf_string(key) else { return std::ptr::null() };
    unsafe { CFDictionaryGetValue(dictionary as CFDictionaryRef, key.0) }
}

fn number(value: CFTypeRef) -> Option<i64> {
    if !is(value, unsafe { CFNumberGetTypeID() }) {
        return None;
    }
    let mut out = 0i64;
    let read = unsafe { CFNumberGetValue(value as CFNumberRef, kCFNumberSInt64Type, (&raw mut out).cast()) };
    read.then_some(out)
}

fn string(value: CFTypeRef) -> Option<String> {
    if !is(value, unsafe { CFStringGetTypeID() }) {
        return None;
    }
    let mut buffer = [0 as c_char; PROC_NAME];
    let read =
        unsafe { CFStringGetCString(value as CFStringRef, buffer.as_mut_ptr(), PROC_NAME as _, kCFStringEncodingUTF8) };
    (read != 0).then(|| unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_string_lossy().into_owned())
}

fn accelerators(each: &mut dyn FnMut(u32)) {
    let matching = unsafe { IOServiceMatching(c"IOAccelerator".as_ptr()) };
    if matching.is_null() {
        return;
    }
    let mut it = 0u32;
    if unsafe { IOServiceGetMatchingServices(0, matching, &mut it) } != 0 {
        return;
    }
    loop {
        let entry = unsafe { IOIteratorNext(it) };
        if entry == 0 {
            break;
        }
        each(entry);
        unsafe { IOObjectRelease(entry) };
    }
    unsafe { IOObjectRelease(it) };
}

/// The GPU's utilisation, as its driver reports it.
pub fn gpu_utilisation() -> Option<f32> {
    let mut found = None;
    accelerators(&mut |entry| {
        if let Some(stats) = property(entry, "PerformanceStatistics") {
            found = found.or(number(get(stats.0, "Device Utilization %")).map(|n| n as f32));
        }
    });
    found
}

/// Each process's GPU time so far, in nanoseconds: the driver's clients (one or more a
/// process) say who made them and the time their command buffers took.
pub fn gpu_times() -> Option<HashMap<i32, u64>> {
    let mut times: HashMap<i32, u64> = HashMap::new();
    let mut any = false;
    accelerators(&mut |entry| {
        any = true;
        let mut it = 0u32;
        if unsafe { IORegistryEntryGetChildIterator(entry, c"IOService".as_ptr(), &mut it) } != 0 {
            return;
        }
        loop {
            let client = unsafe { IOIteratorNext(it) };
            if client == 0 {
                break;
            }
            let creator = property(client, "IOUserClientCreator").and_then(|c| string(c.0));
            let pid = creator.as_deref().and_then(creator_pid);
            if let (Some(pid), Some(usage)) = (pid, property(client, "AppUsage"))
                && is(usage.0, unsafe { CFArrayGetTypeID() })
            {
                let array = usage.0 as CFArrayRef;
                for at in 0..unsafe { CFArrayGetCount(array) } {
                    let item = unsafe { CFArrayGetValueAtIndex(array, at) };
                    if let Some(time) = number(get(item, "accumulatedGPUTime")) {
                        *times.entry(pid).or_default() += time.max(0) as u64;
                    }
                }
            }
            unsafe { IOObjectRelease(client) };
        }
        unsafe { IOObjectRelease(it) };
    });
    any.then_some(times)
}

/// `pid 425, WindowServer` → 425.
pub fn creator_pid(creator: &str) -> Option<i32> {
    creator.strip_prefix("pid ")?.split(',').next()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::creator_pid;
    use crate::diagnostics::processes::Reader;

    #[test]
    fn a_gpu_client_is_named_by_the_process_that_made_it() {
        assert_eq!(creator_pid("pid 425, WindowServer"), Some(425));
        assert_eq!(creator_pid("WindowServer"), None);
    }

    #[test]
    fn the_app_is_read_with_its_threads() {
        let mut reader = Reader::new(std::env::temp_dir().join("pumi-no-server-here"));
        reader.read();
        let read = reader.read();
        let app = read.list.iter().find(|p| p.role == "app").expect("the app is always listed");
        assert_eq!(app.pid, std::process::id() as i32);
        assert!(app.rss.is_some_and(|rss| rss > 0) && app.threads.is_some_and(|n| n > 0));
        assert!(read.threads.iter().map(|t| t.count).sum::<u32>() > 0);
        assert!(read.list.iter().all(|p| p.role != "server"), "no lock file, no server");
    }
}
