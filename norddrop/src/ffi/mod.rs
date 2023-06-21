pub mod types;
mod version;

use std::{
    ffi::{CStr, CString},
    panic,
    sync::{Mutex, Once},
};

use libc::c_char;
use slog::{error, o, warn, Drain, Logger};

use self::types::{
    norddrop_event_cb, norddrop_log_level, norddrop_logger_cb, norddrop_pubkey_cb, norddrop_result,
};
use crate::{
    device::{NordDropFFI, Result as DevResult},
    ffi::types::PanicError,
};

/// Cehck if res is ok, else return early by converting Error into
/// norddrop_result
macro_rules! ffi_try {
    ($expr:expr $(,)?) => {
        match $expr {
            Ok(v) => v,
            Err(e) => return Into::<norddrop_result>::into(e),
        }
    };
}

/// cbindgen:ignore
static PANIC_HOOK: Once = Once::new();

extern "C" {
    fn fortify_source();
}

#[allow(non_camel_case_types)]
pub struct norddrop(Mutex<NordDropFFI>);

/// @brief Initialize a new transfer with the peer and descriptors provided and
/// return the transfer ID
///
/// @param dev   Pointer to the instance
/// @param peer  Peer address
/// @param descriptors   JSON descriptors
/// @return char*  Transfer ID
///
/// descriptors format:
/// [
///  {
///    "path": "/path/to/file",
///  },
/// {
///   "path": "/path/to/dir",
/// }
/// ]
///
/// On Android due to limitations we must also accept a file descriptor
/// {
///  [
///  "path": "/path/to/file",
///  "fd" 1234
///  ]
/// }
#[no_mangle]
pub extern "C" fn norddrop_new_transfer(
    dev: &norddrop,
    peer: *const c_char,
    descriptors: *const c_char,
) -> *mut c_char {
    let res = panic::catch_unwind(|| {
        let mut dev = dev.0.lock().expect("lock instance");

        if peer.is_null() {
            return Err(norddrop_result::NORDDROP_RES_INVALID_STRING);
        }

        let peer = unsafe { CStr::from_ptr(peer) }.to_str()?;

        if descriptors.is_null() {
            return Err(norddrop_result::NORDDROP_RES_INVALID_STRING);
        }

        let descriptors = unsafe { CStr::from_ptr(descriptors) }.to_str()?;

        let xfid = dev.new_transfer(peer, descriptors)?;

        Ok(xfid.to_string().into_bytes())
    });

    // catch_unwind catches panics as errors and everything else goes to Ok
    match res {
        Ok(Ok(xfid)) => new_unmanaged_str(&xfid),
        _ => std::ptr::null_mut(),
    }
}

/// @brief Destroy the libdrop instance
///
/// @param dev   Pointer to the instance
#[no_mangle]
pub extern "C" fn norddrop_destroy(dev: *mut norddrop) {
    if !dev.is_null() {
        unsafe { Box::from_raw(dev) };
    }
}

/// @brief Download a file from the peer
///
/// @param dev   Pointer to the instance
/// @param xfid  Transfer ID
/// @param fid   File ID
/// @param dst   Destination path
/// @return enum norddrop_result   Result of the operation
#[no_mangle]
pub extern "C" fn norddrop_download(
    dev: &norddrop,
    xfid: *const c_char,
    fid: *const c_char,
    dst: *const c_char,
) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let mut dev = ffi_try!(dev
            .0
            .lock()
            .map_err(|_| norddrop_result::NORDDROP_RES_ERROR));

        let str_xfid = {
            if xfid.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }
            let cstr_xfid = unsafe { CStr::from_ptr(xfid) };
            ffi_try!(cstr_xfid.to_str())
        };

        let str_fid = {
            if fid.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }
            let cstr_fid = unsafe { CStr::from_ptr(fid) };
            ffi_try!(cstr_fid.to_str())
        };

        let str_dst = {
            if dst.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }
            let cstr_dst = unsafe { CStr::from_ptr(dst) };
            ffi_try!(cstr_dst.to_str())
        };

        dev.download(
            ffi_try!(str_xfid
                .to_string()
                .parse()
                .map_err(|_| norddrop_result::NORDDROP_RES_BAD_INPUT)),
            ffi_try!(str_fid
                .to_string()
                .parse()
                .map_err(|_| norddrop_result::NORDDROP_RES_BAD_INPUT)),
            str_dst.to_string(),
        )
        .norddrop_log_result(&dev.logger, "norddrop_download")
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

/// @brief  Cancel a transfer from either side
///
/// @param dev   Pointer to the instance
/// @param xfid  Transfer ID
/// @return enum norddrop_result   Result of the operation
#[no_mangle]
pub extern "C" fn norddrop_cancel_transfer(dev: &norddrop, xfid: *const c_char) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let str_xfid = {
            if xfid.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }
            let cstr_xfid = unsafe { CStr::from_ptr(xfid) };
            ffi_try!(cstr_xfid.to_str())
        };

        let mut dev = ffi_try!(dev
            .0
            .lock()
            .map_err(|_| norddrop_result::NORDDROP_RES_ERROR));

        dev.cancel_transfer(ffi_try!(str_xfid
            .to_string()
            .parse()
            .map_err(|_| norddrop_result::NORDDROP_RES_BAD_INPUT)))
            .norddrop_log_result(
                &dev.logger,
                &format!("norddrop_cancel_transfer, xfid: {xfid:?}"),
            )
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

/// @brief  Cancel a file from either side
///
/// @param dev   Pointer to the instance
/// @param xfid  Transfer ID
/// @param fid   File ID
/// @return enum norddrop_result   Result of the operation
#[no_mangle]
pub extern "C" fn norddrop_cancel_file(
    dev: &norddrop,
    xfid: *const c_char,
    fid: *const c_char,
) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let str_xfid = {
            if xfid.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }
            let cstr_xfid = unsafe { CStr::from_ptr(xfid) };
            ffi_try!(cstr_xfid.to_str())
        };

        let str_fid = {
            if fid.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }

            let cstr_fid = unsafe { CStr::from_ptr(fid) };
            ffi_try!(cstr_fid.to_str())
        };

        let mut dev = ffi_try!(dev
            .0
            .lock()
            .map_err(|_| norddrop_result::NORDDROP_RES_ERROR));

        dev.cancel_file(
            ffi_try!(str_xfid
                .to_string()
                .parse()
                .map_err(|_| norddrop_result::NORDDROP_RES_BAD_INPUT)),
            ffi_try!(str_fid
                .parse()
                .map_err(|_| norddrop_result::NORDDROP_RES_BAD_INPUT)),
        );

        norddrop_result::NORDDROP_RES_OK
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

/// @brief  Reject a file from either side
///
/// @param dev   Pointer to the instance
/// @param xfid  Transfer ID
/// @param fid   File ID
/// @return enum norddrop_result   Result of the operation
#[no_mangle]
pub unsafe extern "C" fn norddrop_reject_file(
    dev: &norddrop,
    xfid: *const c_char,
    fid: *const c_char,
) -> norddrop_result {
    let result = panic::catch_unwind(|| {
        let xfid = {
            if xfid.is_null() {
                return Err(norddrop_result::NORDDROP_RES_INVALID_STRING);
            }

            CStr::from_ptr(xfid)
                .to_str()?
                .parse()
                .map_err(|_| norddrop_result::NORDDROP_RES_BAD_INPUT)?
        };

        let fid = {
            if fid.is_null() {
                return Err(norddrop_result::NORDDROP_RES_INVALID_STRING);
            }

            CStr::from_ptr(fid).to_str()?.to_owned()
        };

        let dev = dev
            .0
            .lock()
            .map_err(|_| norddrop_result::NORDDROP_RES_ERROR)?;

        dev.reject_file(xfid, fid)?;

        Ok(())
    });

    match result {
        Ok(Ok(())) => norddrop_result::NORDDROP_RES_OK,
        Ok(Err(err)) => err,
        Err(_) => norddrop_result::NORDDROP_RES_ERROR,
    }
}

/// @brief   Start libdrop
///
/// @param dev   Pointer to the instance
/// @param listen_addr   Address to listen on
/// @param config  JSON configuration
/// @return enum norddrop_result   Result of the operation
///
/// configuration parameters:
///
/// dir_depth_limit - if the tree contains more levels then the error is
/// returned.
///
/// transfer_file_limit - when aggregating files from the path, if this
/// limit is reached, an error is returned.
///
/// req_connection_timeout_ms - timeout value used in connecting to the peer.
/// The formula for retrying is: starting from 0.2 seconds we double it
/// each time until we cap at req_connection_timeout_ms / 10. This is useful
/// when the peer is not responding at all.
///
/// transfer_idle_lifetime_ms - this timeout plays a role in an already
/// established transfer as sometimes one peer might go offline with no notice.
/// This timeout controls the amount of time we will wait for any action from
/// the peer and after that, we will fail the transfer.
///
/// moose_event_path - moose database path.
///
/// storage_path - storage path for persistence engine.
#[no_mangle]
pub extern "C" fn norddrop_start(
    dev: &norddrop,
    listen_addr: *const c_char,
    config: *const c_char,
) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let addr = {
            if listen_addr.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }

            ffi_try!(unsafe { CStr::from_ptr(listen_addr) }.to_str())
        };

        let config = {
            if config.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }

            ffi_try!(unsafe { CStr::from_ptr(config) }.to_str())
        };

        let mut dev = ffi_try!(dev
            .0
            .lock()
            .map_err(|_| norddrop_result::NORDDROP_RES_ERROR));

        dev.start(addr, config)
            .norddrop_log_result(&dev.logger, "norddrop_start")
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

/// @brief Stop norddrop instance.
///
/// @param dev   Pointer to the instance
/// @return enum norddrop_result   Result of the operation
#[no_mangle]
pub extern "C" fn norddrop_stop(dev: &norddrop) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let mut dev = match dev.0.lock() {
            Ok(inst) => inst,
            Err(poisoned) => poisoned.into_inner(),
        };

        dev.stop().norddrop_log_result(&dev.logger, "norddrop_stop")
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

/// @brief Purge transfers from the database
///
/// @param dev   Pointer to the instance
/// @param txids   JSON array of transfer IDs
/// @return enum norddrop_result   Result of the operation
#[no_mangle]
pub extern "C" fn norddrop_purge_transfers(
    dev: &norddrop,
    txids: *const c_char,
) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let mut dev = match dev.0.lock() {
            Ok(inst) => inst,
            Err(poisoned) => poisoned.into_inner(),
        };

        let txids = {
            if txids.is_null() {
                return norddrop_result::NORDDROP_RES_INVALID_STRING;
            }

            ffi_try!(unsafe { CStr::from_ptr(txids) }.to_str())
        };

        dev.purge_transfers(txids)
            .norddrop_log_result(&dev.logger, "norddrop_purge_transfers")
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

/// @brief Purge transfers from the database until the given timestamp
///
/// @param dev   Pointer to the instance
/// @param until_timestamp   Unix timestamp in seconds
/// @return enum norddrop_result   Result of the operation
#[no_mangle]
pub extern "C" fn norddrop_purge_transfers_until(
    dev: &norddrop,
    until_timestamp: std::ffi::c_longlong,
) -> norddrop_result {
    let result = panic::catch_unwind(move || {
        let mut dev = match dev.0.lock() {
            Ok(inst) => inst,
            Err(poisoned) => poisoned.into_inner(),
        };

        dev.purge_transfers_until(until_timestamp)
            .norddrop_log_result(&dev.logger, "norddrop_purge_transfers_until")
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

/// @brief Get transfers from the database
///
/// @param dev  Pointer to the instance
/// @param since_timestamp   Timestamp in seconds
/// @return char*  JSON array of transfers
///
/// JSON example from the sender side:
///  {
///      "id": "b49fc2f8-ce2d-41ac-a081-96a4d760899e",
///      "peer_id": "192.168.0.0",
///      "created_at": 1686651025988,
///      "states": [
///          {
///              "created_at": 1686651026008,
///              "state": "cancel",
///              "by_peer": true
///          }
///      ],
///      "type": "outgoing",
///      "paths": [
///          {
///              "transfer_id": "b49fc2f8-ce2d-41ac-a081-96a4d760899e",
///              "base_path": "/home/user/Pictures",
///              "relative_path": "doggo.jpg",
///              "file_id": "Unu_l4PVyu15-RsdVL9IOQvaKQdqcqUy7F9EpvP-CrY",
///              "bytes": 29852,
///              "created_at": 1686651025988,
///              "states": [
///                  {
///                      "created_at": 1686651025991,
///                      "state": "pending"
///                  },
///                  {
///                      "created_at": 1686651025997,
///                      "state": "started",
///                      "bytes_sent": 0
///                  },
///                  {
///                      "created_at": 1686651026002,
///                      "state": "completed"
///                  }
///              ]
///          }
///      ]
///  }
///
/// JSON example from the receiver side:
/// {
///     "id": "b49fc2f8-ce2d-41ac-a081-96a4d760899e",
///     "peer_id": "172.17.0.1",
///     "created_at": 1686651025988,
///     "states": [
///         {
///             "created_at": 1686651026007,
///             "state": "cancel",
///             "by_peer": false
///         }
///     ],
///     "type": "outgoing",
///     "paths": [
///         {
///             "transfer_id": "b49fc2f8-ce2d-41ac-a081-96a4d760899e",
///             "relative_path": "doggo.jpg",
///             "file_id": "Unu_l4PVyu15-RsdVL9IOQvaKQdqcqUy7F9EpvP-CrY",
///             "bytes": 29852,
///             "created_at": 1686651025988,
///             "states": [
///                 {
///                     "created_at": 1686651025992,
///                     "state": "pending"
///                 },
///                 {
///                     "created_at": 1686651026000,
///                     "state": "started",
///                     "base_dir": "/root",
///                     "bytes_received": 0
///                 },
///                 {
///                     "created_at": 1686651026003,
///                     "state": "completed",
///                     "final_path": "/root/doggo.jpg"
///                 }
///             ]
///         }
///     ]
/// }
#[no_mangle]
pub extern "C" fn norddrop_get_transfers_since(
    dev: &norddrop,
    since_timestamp: std::ffi::c_longlong,
) -> *mut c_char {
    let res = panic::catch_unwind(move || {
        let mut dev = match dev.0.lock() {
            Ok(inst) => inst,
            Err(poisoned) => poisoned.into_inner(),
        };

        let transfers = dev.transfers_since(since_timestamp)?;

        Ok::<Vec<u8>, norddrop_result>(transfers.into_bytes())
    });

    match res {
        Ok(Ok(transfers)) => new_unmanaged_str(&transfers),
        _ => std::ptr::null_mut(),
    }
}

/// @brief Create a new instance of norddrop. This is a required step to work
/// with API further.
///
/// @param dev  Pointer to the pointer to the instance. The pointer will be
///             allocated by the function and should be freed by the caller
///             using norddrop_destroy()
/// @param event_cb     Event callback
/// @param log_level    Log level
/// @param logger_cb    Logger callback
/// @param pubkey_cb    Fetch peer public key callback. It is used to request
/// the app to provide the peer’s public key or the node itself. The callback
/// provides two parameters, `const char *ip` which is a string
/// representation of the peer’s IP address, and `char *pubkey` which is
/// preallocated buffer of size 32 into which the app should write the public
/// key as bytes. The app returns the status of the callback call, 0 on
/// success and a non-zero value to indicate that the key could not be
/// provided. Note that it’s not BASE64, it must be decoded if it is beforehand.
/// @param privkey     32bytes private key. Note that it’s not BASE64, it must
/// be decoded if it is beforehand. @return NORDDROP_RES_OK on success, error
/// code otherwise
#[no_mangle]
pub extern "C" fn norddrop_new(
    dev: *mut *mut norddrop,
    event_cb: norddrop_event_cb,
    log_level: norddrop_log_level,
    logger_cb: norddrop_logger_cb,
    pubkey_cb: norddrop_pubkey_cb,
    privkey: *const c_char,
) -> norddrop_result {
    unsafe {
        fortify_source();
    }

    let logger = Logger::root(logger_cb.filter_level(log_level.into()).fuse(), o!());

    PANIC_HOOK.call_once(|| {
        let logger = logger.clone();

        panic::set_hook(Box::new(move |info: &panic::PanicInfo| {
            error!(logger, "{}", info);

            let res = CString::new(
                serde_json::to_string(&PanicError::from(info))
                    .unwrap_or_else(|_| String::from("event_to_json error")),
            );

            match res {
                Ok(s) => unsafe {
                    let callback = event_cb.callback();
                    let callback_data = event_cb.callback_data();

                    (callback)(callback_data, s.as_ptr())
                },
                Err(e) => warn!(logger, "Failed to create CString: {}", e),
            }
        }));
    });

    let result = panic::catch_unwind(move || {
        if privkey.is_null() {
            return norddrop_result::NORDDROP_RES_INVALID_PRIVKEY;
        }
        let mut privkey_bytes = [0u8; drop_auth::SECRET_KEY_LENGTH];
        unsafe {
            privkey_bytes
                .as_mut_ptr()
                .copy_from_nonoverlapping(privkey as *const _, drop_auth::SECRET_KEY_LENGTH);
        }
        let privkey = drop_auth::SecretKey::from(privkey_bytes);

        let drive = ffi_try!(NordDropFFI::new(event_cb, pubkey_cb, privkey, logger));
        unsafe { *dev = Box::into_raw(Box::new(norddrop(Mutex::new(drive)))) };

        norddrop_result::NORDDROP_RES_OK
    });

    result.unwrap_or(norddrop_result::NORDDROP_RES_ERROR)
}

impl slog::Drain for norddrop_logger_cb {
    type Ok = ();
    type Err = ();

    fn log(&self, record: &slog::Record, _: &slog::OwnedKVList) -> Result<Self::Ok, Self::Err> {
        if !self.is_enabled(record.level()) {
            return Ok(());
        }

        let file = record.location().file;
        let line = record.location().line;

        let msg = format!("{file}:{line} {}", record.msg());
        if let Ok(cstr) = CString::new(msg) {
            unsafe { (self.cb)(self.ctx, record.level().into(), cstr.as_ptr()) };
        }

        Ok(())
    }
}

trait FFILog {
    fn norddrop_log_result(self, logger: &Logger, caller: &str) -> norddrop_result;
}

impl FFILog for DevResult {
    fn norddrop_log_result(self, logger: &Logger, caller: &str) -> norddrop_result {
        let res = norddrop_result::from(self);
        match res {
            norddrop_result::NORDDROP_RES_OK => {}
            _ => error!(logger, "{:?}: {res:?}", caller),
        };
        res
    }
}

fn new_unmanaged_str(bytes: &[u8]) -> *mut c_char {
    let ptr = unsafe { libc::calloc(bytes.len() + 1, std::mem::size_of::<c_char>()) };
    if ptr.is_null() {
        return std::ptr::null_mut();
    };

    unsafe { ptr.copy_from_nonoverlapping(bytes.as_ptr() as *const _, bytes.len()) };

    ptr as *mut c_char
}
