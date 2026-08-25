use std::{
    collections::HashMap, ffi::c_void, fs::File, io::{BufReader, prelude::*}, ptr::null_mut,
};

use windows::{
    core::HSTRING,
    Win32::{
        Foundation::*,
        Storage::FileSystem::*,
        System::IO::*,
        System::Threading::INFINITE,
    },
};

const BUF_SIZE: usize = 1024;

#[repr(C)]
struct DirectoryContext {
    overlapped: OVERLAPPED,
    handle: HANDLE,
    path: String,
    buffer: [u8; BUF_SIZE]
}

/* 
    Error Codes:
        1 : Failed to open file
        2 : Failed empty vector
*/
fn read_conf(config: &str) -> Result<Vec<String>, u32> {
    let file = match File::open(config) {
        Ok(file) => file,
        Err(_) => {
            println!("[!] Failed to open file: {}", config);
            return Err(1);
        }
    };
    let buffer = BufReader::new(file);
    let mut monitoring_list: Vec<String> = Vec::new();
    for line in buffer.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => {
                println!("[!] Error reading line from config.");
                continue;
            }
        };
        monitoring_list.push(line);
    }
    if monitoring_list.is_empty() {
        return Err(2);
    }
    Ok(monitoring_list)
}

/* 
    Error codes:
        1 : Empty handle vector
*/
fn directory_handles(monitoring: Vec<String>) -> Result<HashMap<String, HANDLE>, u32> {
    let mut handles = HashMap::new();
    unsafe {
        for dir in monitoring {
            match CreateFileW(
                &HSTRING::from(&dir),
                GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
                None
            ) {
                Ok(h) => handles.insert(dir.clone(), h),
                Err(e) => {
                    println!("[!] CreateFileW failed with error: {}", e);
                    continue;
                }
            };
        }
    }

    if handles.is_empty() {
        println!("[!] Failed to get handles to directories from config");
        return Err(1);
    }
    Ok(handles)
}

fn change_detection(handles: &HashMap<String, HANDLE>) {
    let mut contexts: Vec<*mut DirectoryContext> = Vec::new();
    unsafe {
        let iocp = match CreateIoCompletionPort(INVALID_HANDLE_VALUE, None, 0, 0) {
            Ok(h) => h,
            Err(e) => {
                println!("[!] CreateIoCompletionPort failed with error: {}", e);
                return;
            }
        };

        for h in handles {
            if let Err(e) = CreateIoCompletionPort(*h.1, Some(iocp), 0, 0) {
                println!("[!] CreateIoCompletionPort 2 failed with error: {}", e);
                return;
            }
            
            let ctx = Box::new(DirectoryContext {
                overlapped: std::mem::zeroed(),
                handle: *h.1,
                path: (*h.0.clone()).to_string(),
                buffer: [0; BUF_SIZE],
            });
            let ctx_ptr = Box::into_raw(ctx);
            contexts.push(ctx_ptr);

            if let Err(e) = ReadDirectoryChangesW(
                (*ctx_ptr).handle,
                (*ctx_ptr).buffer.as_mut_ptr() as *mut c_void,
                (*ctx_ptr).buffer.len() as u32,
                true,
                FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME | FILE_NOTIFY_CHANGE_SIZE | FILE_NOTIFY_CHANGE_LAST_WRITE,
                None,
                Some(&mut (*ctx_ptr).overlapped),
                None,
            ) {
                println!("[!] ReadDirectoryChangesW failed with error: {}", e);
                return;
            }
        }
        println!("[+] Directory listener successfully started.");

        loop {
            let mut bytes_transferred = 0;
            let mut completion_key = 0;
            let mut overlapped: *mut OVERLAPPED = null_mut();

            match GetQueuedCompletionStatus(
                iocp, 
                &mut bytes_transferred, 
                &mut completion_key, 
                &mut overlapped, 
                INFINITE,
            ) {
                Ok(_) => {
                    if overlapped.is_null() {
                        println!("[!] GetQueuedCompletionStatus returned null context");
                        return;
                    }

                    let ctx = &mut *(overlapped as *mut DirectoryContext);
                    action_change(ctx);

                    ctx.overlapped = std::mem::zeroed();

                    if let Err(e) = ReadDirectoryChangesW(
                        ctx.handle,
                        ctx.buffer.as_mut_ptr() as *mut c_void,
                        ctx.buffer.len() as u32,
                        true,
                        FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME | FILE_NOTIFY_CHANGE_SIZE | FILE_NOTIFY_CHANGE_LAST_WRITE,
                        None,
                        Some(&mut ctx.overlapped),
                        None,
                    ) {
                        println!("[!] ReadDirectoryChangesW failed with error: {}", e);
                        return;
                    }
                }
                Err(e) => {
                    println!("[!] GetQueuedCompletionStatus failed with error: {}", e);
                    return;
                }
            }
        }
    }
}

fn action_change(ctx: &mut DirectoryContext) {
    unsafe {
        let info = ctx.buffer.as_ptr() as *const FILE_NOTIFY_INFORMATION;
        let path: &String = &ctx.path;
        let filename = std::slice::from_raw_parts((*info).FileName.as_ptr(), ((*info).FileNameLength/2) as usize);
        let filename = String::from_utf16(filename).unwrap();
        let action = match (*info).Action.0 {
            1 => "FILE_ACTION_ADDED",
            2 => "FILE_ACTION_REMOVED",
            3 => "FILE_ACTION_MODIFIED",
            4 => "FILE_ACTION_RENAMED_OLD_NAME",
            5 => "FILE_ACTION_RENAMED_NEW_NAME",
            _ => "Unaccounted for FILE_ACTION",
        };
        println!("\t[+] {}: {}\\{}", action, path, filename);
    }
}

fn main() {
    println!("Hello, world!");
    let monitoring_list = match read_conf("./monitoring.conf") {
        Ok(res) => res,
        Err(e) => {
            println!("[!] read_conf failed with error: {}", e);
            return;
        }
    };
    println!("[+] Successfully read config file with lines: {:?}", monitoring_list);

    let d_handles = match directory_handles(monitoring_list) {
        Ok(res) => res,
        Err(e) => {
            println!("[!] directory_handles failed with error: {}", e);
            return;
        }
    };
    println!("[+] Successfully opened handles to directories: {:?}", d_handles);

    change_detection(&d_handles);
}
