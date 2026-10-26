//! macOS: `proc_pidinfo`.
pub fn name(pid: u32) -> Option<String> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    let ret = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    if ret != size {
        return None;
    }
    let end = info.pbi_comm.iter().position(|&b| b == 0).unwrap_or(info.pbi_comm.len());
    (end > 0).then(|| info.pbi_comm[..end].iter().map(|&b| b as u8 as char).collect())
}
pub fn cwd(pid: u32) -> Option<String> {
    let mut info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_vnodepathinfo>() as libc::c_int;
    let ret = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            (&mut info as *mut libc::proc_vnodepathinfo).cast(),
            size,
        )
    };
    if ret != size {
        return None;
    }
    let raw: &[libc::c_char] = unsafe { std::slice::from_raw_parts(info.pvi_cdir.vip_path.as_ptr().cast(), 32 * 32) };
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    let path: Vec<u8> = raw[..end].iter().map(|&b| b as u8).collect();
    (!path.is_empty()).then(|| String::from_utf8_lossy(&path).into_owned())
}
