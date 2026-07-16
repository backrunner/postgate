use crate::error::{PostGateError, Result};
use block2::RcBlock;
use core_foundation_sys::array::{
    CFArrayGetCount, CFArrayGetTypeID, CFArrayGetValueAtIndex, CFArrayRef,
};
use core_foundation_sys::base::{CFGetTypeID, CFRelease, CFTypeRef};
use core_foundation_sys::error::CFErrorRef;
use core_foundation_sys::string::{
    kCFStringEncodingUTF8, CFStringCreateWithCString, CFStringGetTypeID, CFStringRef,
};
use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::AnyThread;
use objc2_cloud_kit::{
    CKAsset, CKContainer, CKDatabase, CKErrorCode, CKErrorRetryAfterKey, CKFetchRecordsOperation,
    CKRecord, CKRecordID, CKRecordValue,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSNumber, NSString, NSURL};
use std::ffi::{c_void, CString};
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

pub const CONTAINER_ID: &str = "iCloud.com.alkinum.postgate";
pub const LOCATION: &str = "cloudkit://iCloud.com.alkinum.postgate/private/profile-v1";

const RECORD_TYPE: &str = "PostGateProfile";
const RECORD_NAME: &str = "profile-v1";
const PAYLOAD_FIELD: &str = "payload";
const METADATA_TIMEOUT: Duration = Duration::from_secs(30);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_RETRY_ATTEMPTS: usize = 3;
const MAX_PROFILE_BYTES: usize = 40 * 1024 * 1024;
const CONTAINERS_ENTITLEMENT: &str = "com.apple.developer.icloud-container-identifiers";
const SERVICES_ENTITLEMENT: &str = "com.apple.developer.icloud-services";
const ENVIRONMENT_ENTITLEMENT: &str = "com.apple.developer.icloud-container-environment";

type SecTaskRef = *const c_void;

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn SecTaskCreateFromSelf(allocator: *const c_void) -> SecTaskRef;
    fn SecTaskCopyValueForEntitlement(
        task: SecTaskRef,
        entitlement: CFStringRef,
        error: *mut CFErrorRef,
    ) -> CFTypeRef;
}

#[derive(Debug)]
pub struct RemoteProfile {
    pub payload: Vec<u8>,
    pub change_tag: String,
}

pub fn is_available() -> bool {
    entitlement_array_contains(CONTAINERS_ENTITLEMENT, CONTAINER_ID)
        && entitlement_array_contains(SERVICES_ENTITLEMENT, "CloudKit")
        && entitlement_string_matches(ENVIRONMENT_ENTITLEMENT, &["Development", "Production"])
        && current_executable_has_embedded_profile()
}

pub async fn change_tag() -> Result<Option<String>> {
    ensure_available()?;
    run_blocking(|| with_fetch_retries(fetch_change_tag_once)).await
}

pub async fn pull() -> Result<RemoteProfile> {
    ensure_available()?;
    run_blocking(|| with_fetch_retries(fetch_profile_once))
        .await?
        .ok_or_else(|| PostGateError::NotFound("No CloudKit profile has been uploaded".into()))
}

pub async fn push(payload: Vec<u8>, expected_change_tag: Option<String>) -> Result<String> {
    ensure_available()?;
    validate_payload_size(payload.len()).map_err(NativeError::into_postgate)?;
    run_blocking(move || push_blocking(payload, expected_change_tag)).await
}

fn ensure_available() -> Result<()> {
    if is_available() {
        Ok(())
    } else {
        Err(PostGateError::InvalidState(
            "CloudKit is unavailable because this app is not signed with a valid embedded PostGate provisioning profile"
                .into(),
        ))
    }
}

fn current_executable_has_embedded_profile() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|executable| embedded_profile_path(&executable))
        .is_some_and(|profile| profile.is_file())
}

fn embedded_profile_path(executable: &Path) -> Option<PathBuf> {
    let macos = executable.parent()?;
    if macos.file_name()?.to_str()? != "MacOS" {
        return None;
    }
    let contents = macos.parent()?;
    if contents.file_name()?.to_str()? != "Contents" {
        return None;
    }
    Some(contents.join("embedded.provisionprofile"))
}

fn copy_entitlement_value(entitlement: &str) -> Option<CFTypeRef> {
    let entitlement = CString::new(entitlement).ok()?;
    unsafe {
        let task = SecTaskCreateFromSelf(ptr::null());
        if task.is_null() {
            return None;
        }
        let entitlement =
            CFStringCreateWithCString(ptr::null(), entitlement.as_ptr(), kCFStringEncodingUTF8);
        if entitlement.is_null() {
            CFRelease(task as CFTypeRef);
            return None;
        }
        let value = SecTaskCopyValueForEntitlement(task, entitlement, ptr::null_mut());
        CFRelease(entitlement as CFTypeRef);
        CFRelease(task as CFTypeRef);
        (!value.is_null()).then_some(value)
    }
}

fn entitlement_array_contains(entitlement: &str, expected: &str) -> bool {
    let Some(value) = copy_entitlement_value(entitlement) else {
        return false;
    };
    let Ok(expected) = CString::new(expected) else {
        unsafe { CFRelease(value) };
        return false;
    };

    unsafe {
        if CFGetTypeID(value) != CFArrayGetTypeID() {
            CFRelease(value);
            return false;
        }
        let expected =
            CFStringCreateWithCString(ptr::null(), expected.as_ptr(), kCFStringEncodingUTF8);
        if expected.is_null() {
            CFRelease(value);
            return false;
        }
        let array = value as CFArrayRef;
        let found = (0..CFArrayGetCount(array)).any(|index| {
            core_foundation_sys::base::CFEqual(
                CFArrayGetValueAtIndex(array, index) as CFTypeRef,
                expected as CFTypeRef,
            ) != 0
        });
        CFRelease(expected as CFTypeRef);
        CFRelease(value);
        found
    }
}

fn entitlement_string_matches(entitlement: &str, expected: &[&str]) -> bool {
    let Some(value) = copy_entitlement_value(entitlement) else {
        return false;
    };

    unsafe {
        if CFGetTypeID(value) != CFStringGetTypeID() {
            CFRelease(value);
            return false;
        }
        let found = expected.iter().any(|candidate| {
            let Ok(candidate) = CString::new(*candidate) else {
                return false;
            };
            let candidate =
                CFStringCreateWithCString(ptr::null(), candidate.as_ptr(), kCFStringEncodingUTF8);
            if candidate.is_null() {
                return false;
            }
            let matches = core_foundation_sys::base::CFEqual(value, candidate as CFTypeRef) != 0;
            CFRelease(candidate as CFTypeRef);
            matches
        });
        CFRelease(value);
        found
    }
}

async fn run_blocking<T, F>(operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> std::result::Result<T, NativeError> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| PostGateError::Storage(format!("CloudKit worker failed: {error}")))?
        .map_err(NativeError::into_postgate)
}

#[allow(deprecated)]
fn fetch_change_tag_once() -> std::result::Result<Option<String>, NativeError> {
    autoreleasepool(|_| {
        let database = private_database()?;
        let record_id = record_id();
        let record_ids = NSArray::from_retained_slice(&[record_id]);
        let desired_keys: Retained<NSArray<NSString>> = NSArray::from_retained_slice(&[]);
        let operation = unsafe {
            CKFetchRecordsOperation::initWithRecordIDs(
                CKFetchRecordsOperation::alloc(),
                &record_ids,
            )
        };
        let (sender, receiver) = mpsc::channel();
        let per_record_sender = sender.clone();
        let record_callback_seen = Arc::new(AtomicBool::new(false));
        let per_record_callback_seen = record_callback_seen.clone();
        let completion = RcBlock::new(
            move |record: *mut CKRecord, _record_id: *mut CKRecordID, error: *mut NSError| {
                per_record_callback_seen.store(true, Ordering::Release);
                let result = match native_error(error) {
                    Some(error) if error.is_unknown_item() => Ok(None),
                    Some(error) => Err(error),
                    None if record.is_null() => Err(NativeError::Bridge(
                        "CloudKit returned an empty record".into(),
                    )),
                    None => unsafe { record.as_ref() }
                        .and_then(|record| unsafe { record.recordChangeTag() })
                        .map(|value| Some(value.to_string()))
                        .ok_or_else(|| {
                            NativeError::Bridge(
                                "CloudKit profile metadata has no change tag".into(),
                            )
                        }),
                };
                let _ = per_record_sender.send(result);
            },
        );
        let operation_completion = RcBlock::new(
            move |_records: *mut NSDictionary<CKRecordID, CKRecord>, error: *mut NSError| {
                if !record_callback_seen.load(Ordering::Acquire) {
                    let result = Err(native_error(error).unwrap_or_else(|| {
                        NativeError::Bridge(
                            "CloudKit fetch completed without returning the requested record"
                                .into(),
                        )
                    }));
                    let _ = sender.send(result);
                }
            },
        );

        unsafe {
            operation.setDesiredKeys(Some(&desired_keys));
            operation.setPerRecordCompletionBlock(Some(&completion));
            operation.setFetchRecordsCompletionBlock(Some(&operation_completion));
            database.addOperation(&operation);
        }
        receive_fetch_result(&operation, receiver, METADATA_TIMEOUT)
    })
}

#[allow(deprecated)]
fn fetch_profile_once() -> std::result::Result<Option<RemoteProfile>, NativeError> {
    autoreleasepool(|_| {
        let database = private_database()?;
        let record_id = record_id();
        let record_ids = NSArray::from_retained_slice(&[record_id]);
        let payload_key = NSString::from_str(PAYLOAD_FIELD);
        let desired_keys = NSArray::from_retained_slice(&[payload_key]);
        let operation = unsafe {
            CKFetchRecordsOperation::initWithRecordIDs(
                CKFetchRecordsOperation::alloc(),
                &record_ids,
            )
        };
        let (sender, receiver) = mpsc::channel();
        let per_record_sender = sender.clone();
        let record_callback_seen = Arc::new(AtomicBool::new(false));
        let per_record_callback_seen = record_callback_seen.clone();
        let completion = RcBlock::new(
            move |record: *mut CKRecord, _record_id: *mut CKRecordID, error: *mut NSError| {
                per_record_callback_seen.store(true, Ordering::Release);
                autoreleasepool(|_| {
                    let result = match native_error(error) {
                        Some(error) if error.is_unknown_item() => Ok(None),
                        Some(error) => Err(error),
                        None => unsafe { record.as_ref() }
                            .ok_or_else(|| {
                                NativeError::Bridge("CloudKit returned an empty record".into())
                            })
                            .and_then(remote_profile_from_record)
                            .map(Some),
                    };
                    let _ = per_record_sender.send(result);
                });
            },
        );
        let operation_completion = RcBlock::new(
            move |_records: *mut NSDictionary<CKRecordID, CKRecord>, error: *mut NSError| {
                if !record_callback_seen.load(Ordering::Acquire) {
                    let result = Err(native_error(error).unwrap_or_else(|| {
                        NativeError::Bridge(
                            "CloudKit fetch completed without returning the requested record"
                                .into(),
                        )
                    }));
                    let _ = sender.send(result);
                }
            },
        );

        unsafe {
            operation.setDesiredKeys(Some(&desired_keys));
            operation.setPerRecordCompletionBlock(Some(&completion));
            operation.setFetchRecordsCompletionBlock(Some(&operation_completion));
            database.addOperation(&operation);
        }
        receive_fetch_result(&operation, receiver, TRANSFER_TIMEOUT)
    })
}

fn receive_fetch_result<T>(
    operation: &CKFetchRecordsOperation,
    receiver: mpsc::Receiver<std::result::Result<T, NativeError>>,
    timeout: Duration,
) -> std::result::Result<T, NativeError> {
    match receiver.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            operation.cancel();
            Err(NativeError::Timeout)
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(NativeError::Bridge(
            "CloudKit callback disconnected before returning a result".into(),
        )),
    }
}

fn push_blocking(
    payload: Vec<u8>,
    expected_change_tag: Option<String>,
) -> std::result::Result<String, NativeError> {
    validate_payload_size(payload.len())?;
    let mut asset_file = tempfile::Builder::new()
        .prefix("postgate-cloudkit-")
        .suffix(".json")
        .tempfile()
        .map_err(NativeError::Io)?;
    asset_file.write_all(&payload).map_err(NativeError::Io)?;
    asset_file
        .as_file_mut()
        .sync_all()
        .map_err(NativeError::Io)?;
    let asset_path = Arc::new(asset_file.into_temp_path());

    for attempt in 0..MAX_RETRY_ATTEMPTS {
        match push_once(asset_path.clone(), expected_change_tag.clone()) {
            Ok(change_tag) => return Ok(change_tag),
            Err(NativeError::Write(error)) if error.is_ambiguous_write() => {
                return reconcile_ambiguous_write(&payload, *error);
            }
            Err(NativeError::Write(error))
                if error.is_retryable_write() && attempt + 1 < MAX_RETRY_ATTEMPTS =>
            {
                thread::sleep(error.retry_delay(attempt));
            }
            Err(NativeError::Write(error)) => return Err(*error),
            Err(error @ NativeError::Conflict(_)) => {
                if let Some(remote) = with_fetch_retries(fetch_profile_once)? {
                    if remote.payload == payload {
                        return Ok(remote.change_tag);
                    }
                }
                return Err(error);
            }
            Err(error) if error.is_retryable_fetch() && attempt + 1 < MAX_RETRY_ATTEMPTS => {
                thread::sleep(error.retry_delay(attempt));
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("CloudKit push retry loop always returns")
}

#[allow(deprecated)]
fn push_once(
    asset_path: Arc<tempfile::TempPath>,
    expected_change_tag: Option<String>,
) -> std::result::Result<String, NativeError> {
    autoreleasepool(|_| {
        let database = private_database()?;
        let callback_database = database.clone();
        let record_id = record_id();
        let callback_record_id = record_id.clone();
        let record_ids = NSArray::from_retained_slice(&[record_id]);
        let desired_keys: Retained<NSArray<NSString>> = NSArray::from_retained_slice(&[]);
        let operation = unsafe {
            CKFetchRecordsOperation::initWithRecordIDs(
                CKFetchRecordsOperation::alloc(),
                &record_ids,
            )
        };
        let (sender, receiver) = mpsc::channel();
        let save_started = Arc::new(AtomicBool::new(false));
        let callback_save_started = save_started.clone();
        let per_record_sender = sender.clone();
        let record_callback_seen = Arc::new(AtomicBool::new(false));
        let per_record_callback_seen = record_callback_seen.clone();
        let completion = RcBlock::new(
            move |record: *mut CKRecord, _record_id: *mut CKRecordID, error: *mut NSError| {
                per_record_callback_seen.store(true, Ordering::Release);
                autoreleasepool(|_| match native_error(error) {
                    Some(error) if error.is_unknown_item() => {
                        let record = unsafe {
                            CKRecord::initWithRecordType_recordID(
                                CKRecord::alloc(),
                                &NSString::from_str(RECORD_TYPE),
                                &callback_record_id,
                            )
                        };
                        callback_save_started.store(true, Ordering::Release);
                        save_record(
                            &callback_database,
                            &record,
                            asset_path.clone(),
                            per_record_sender.clone(),
                        );
                    }
                    Some(error) => {
                        let _ = per_record_sender.send(Err(error));
                    }
                    None => {
                        let Some(record) = (unsafe { record.as_ref() }) else {
                            let _ = per_record_sender.send(Err(NativeError::Bridge(
                                "CloudKit returned an empty record".into(),
                            )));
                            return;
                        };
                        let remote_change_tag =
                            unsafe { record.recordChangeTag() }.map(|value| value.to_string());
                        if remote_change_tag.as_deref() != expected_change_tag.as_deref() {
                            let _ = per_record_sender.send(Err(NativeError::Conflict(
                                "A newer CloudKit profile exists; pull it before pushing local changes"
                                    .into(),
                            )));
                            return;
                        }
                        callback_save_started.store(true, Ordering::Release);
                        save_record(
                            &callback_database,
                            record,
                            asset_path.clone(),
                            per_record_sender.clone(),
                        );
                    }
                });
            },
        );
        let operation_completion = RcBlock::new(
            move |_records: *mut NSDictionary<CKRecordID, CKRecord>, error: *mut NSError| {
                if !record_callback_seen.load(Ordering::Acquire) {
                    let result = Err(native_error(error).unwrap_or_else(|| {
                        NativeError::Bridge(
                            "CloudKit preflight completed without returning the requested record"
                                .into(),
                        )
                    }));
                    let _ = sender.send(result);
                }
            },
        );

        unsafe {
            operation.setDesiredKeys(Some(&desired_keys));
            operation.setPerRecordCompletionBlock(Some(&completion));
            operation.setFetchRecordsCompletionBlock(Some(&operation_completion));
            database.addOperation(&operation);
        }

        match receiver.recv_timeout(TRANSFER_TIMEOUT) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                operation.cancel();
                if save_started.load(Ordering::Acquire) {
                    Err(NativeError::Write(Box::new(NativeError::Timeout)))
                } else {
                    Err(NativeError::Timeout)
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(NativeError::Bridge(
                "CloudKit callback disconnected before returning a result".into(),
            )),
        }
    })
}

fn save_record(
    database: &CKDatabase,
    record: &CKRecord,
    asset_path: Arc<tempfile::TempPath>,
    sender: mpsc::Sender<std::result::Result<String, NativeError>>,
) {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&asset_path.to_string_lossy()));
    let asset = unsafe { CKAsset::initWithFileURL(CKAsset::alloc(), &url) };
    let value = ProtocolObject::<dyn CKRecordValue>::from_ref(&*asset);
    unsafe {
        record.setObject_forKey(Some(value), &NSString::from_str(PAYLOAD_FIELD));
    }

    let completion = RcBlock::new(move |saved: *mut CKRecord, error: *mut NSError| {
        autoreleasepool(|_| {
            let _keep_asset_alive = &asset_path;
            let result = match native_error(error) {
                Some(error) if error.is_server_record_changed() => Err(NativeError::Conflict(
                    "The CloudKit profile changed while uploading; pull it and retry".into(),
                )),
                Some(error) => Err(NativeError::Write(Box::new(error))),
                None => unsafe { saved.as_ref() }
                    .and_then(|record| unsafe { record.recordChangeTag() })
                    .map(|tag| tag.to_string())
                    .ok_or_else(|| {
                        NativeError::Bridge("CloudKit saved a record without a change tag".into())
                    }),
            };
            let _ = sender.send(result);
        });
    });

    unsafe {
        database.saveRecord_completionHandler(record, &completion);
    }
}

fn reconcile_ambiguous_write(
    payload: &[u8],
    original_error: NativeError,
) -> std::result::Result<String, NativeError> {
    let mut last_fetch_error = None;
    for attempt in 0..MAX_RETRY_ATTEMPTS {
        match fetch_profile_once() {
            Ok(Some(remote)) if remote.payload == payload => return Ok(remote.change_tag),
            Ok(_) => last_fetch_error = None,
            Err(error) => last_fetch_error = Some(error),
        }
        if attempt + 1 < MAX_RETRY_ATTEMPTS {
            thread::sleep(Duration::from_millis(500 * (1 << attempt)));
        }
    }

    let detail = last_fetch_error
        .map(|error| format!("; verification failed: {error}"))
        .unwrap_or_default();
    Err(NativeError::UnknownWriteOutcome(format!(
        "CloudKit upload result is unknown after {original_error}{detail}; pull the remote profile before retrying"
    )))
}

fn with_fetch_retries<T, F>(mut operation: F) -> std::result::Result<T, NativeError>
where
    F: FnMut() -> std::result::Result<T, NativeError>,
{
    for attempt in 0..MAX_RETRY_ATTEMPTS {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) if error.is_retryable_fetch() && attempt + 1 < MAX_RETRY_ATTEMPTS => {
                thread::sleep(error.retry_delay(attempt));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("CloudKit fetch retry loop always returns")
}

fn remote_profile_from_record(
    record: &CKRecord,
) -> std::result::Result<RemoteProfile, NativeError> {
    let value = unsafe { record.objectForKey(&NSString::from_str(PAYLOAD_FIELD)) }
        .ok_or_else(|| NativeError::Bridge("CloudKit profile has no payload asset".into()))?;
    let any_object: &AnyObject = (*value).as_ref();
    let asset = any_object
        .downcast_ref::<CKAsset>()
        .ok_or_else(|| NativeError::Bridge("CloudKit profile payload is not an asset".into()))?;
    let url = unsafe { asset.fileURL() }
        .ok_or_else(|| NativeError::Bridge("CloudKit profile asset has no file URL".into()))?;
    let path = url
        .path()
        .map(|path| path.to_string())
        .ok_or_else(|| NativeError::Bridge("CloudKit profile asset has no local path".into()))?;
    let size = std::fs::metadata(&path).map_err(NativeError::Io)?.len();
    let size = usize::try_from(size).unwrap_or(usize::MAX);
    validate_payload_size(size)?;
    let payload = std::fs::read(path).map_err(NativeError::Io)?;
    let change_tag = unsafe { record.recordChangeTag() }
        .map(|tag| tag.to_string())
        .ok_or_else(|| NativeError::Bridge("CloudKit profile has no change tag".into()))?;

    Ok(RemoteProfile {
        payload,
        change_tag,
    })
}

fn validate_payload_size(size: usize) -> std::result::Result<(), NativeError> {
    if size > MAX_PROFILE_BYTES {
        Err(NativeError::PayloadTooLarge {
            size,
            limit: MAX_PROFILE_BYTES,
        })
    } else {
        Ok(())
    }
}

fn private_database() -> std::result::Result<Retained<CKDatabase>, NativeError> {
    objc2::exception::catch(|| {
        let container =
            unsafe { CKContainer::containerWithIdentifier(&NSString::from_str(CONTAINER_ID)) };
        unsafe { container.privateCloudDatabase() }
    })
    .map_err(|exception| {
        let message = exception
            .map(|exception| exception.to_string())
            .unwrap_or_else(|| "unknown Objective-C exception".into());
        NativeError::Bridge(format!("CloudKit container is unavailable: {message}"))
    })
}

fn record_id() -> Retained<CKRecordID> {
    unsafe { CKRecordID::initWithRecordName(CKRecordID::alloc(), &NSString::from_str(RECORD_NAME)) }
}

fn native_error(error: *mut NSError) -> Option<NativeError> {
    let error = unsafe { error.as_ref() }?;
    let retry_after = error
        .userInfo()
        .objectForKey(unsafe { CKErrorRetryAfterKey })
        .and_then(|value| value.downcast_ref::<NSNumber>().map(NSNumber::doubleValue))
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .map(Duration::from_secs_f64);
    Some(NativeError::CloudKit {
        domain: error.domain().to_string(),
        code: error.code(),
        message: error.localizedDescription().to_string(),
        retry_after,
    })
}

#[derive(Debug)]
enum NativeError {
    CloudKit {
        domain: String,
        code: isize,
        message: String,
        retry_after: Option<Duration>,
    },
    Write(Box<NativeError>),
    Conflict(String),
    UnknownWriteOutcome(String),
    PayloadTooLarge {
        size: usize,
        limit: usize,
    },
    Bridge(String),
    Io(std::io::Error),
    Timeout,
}

impl NativeError {
    fn has_cloudkit_code(&self, expected: CKErrorCode) -> bool {
        match self {
            Self::CloudKit { domain, code, .. } => domain == "CKErrorDomain" && *code == expected.0,
            Self::Write(error) => error.has_cloudkit_code(expected),
            _ => false,
        }
    }

    fn is_unknown_item(&self) -> bool {
        self.has_cloudkit_code(CKErrorCode::UnknownItem)
    }

    fn is_server_record_changed(&self) -> bool {
        self.has_cloudkit_code(CKErrorCode::ServerRecordChanged)
    }

    fn is_retryable_fetch(&self) -> bool {
        [
            CKErrorCode::NetworkUnavailable,
            CKErrorCode::NetworkFailure,
            CKErrorCode::ServiceUnavailable,
            CKErrorCode::RequestRateLimited,
            CKErrorCode::ZoneBusy,
            CKErrorCode::ServerResponseLost,
            CKErrorCode::AccountTemporarilyUnavailable,
        ]
        .into_iter()
        .any(|code| self.has_cloudkit_code(code))
    }

    fn is_retryable_write(&self) -> bool {
        [
            CKErrorCode::NetworkUnavailable,
            CKErrorCode::ServiceUnavailable,
            CKErrorCode::RequestRateLimited,
            CKErrorCode::ZoneBusy,
            CKErrorCode::AccountTemporarilyUnavailable,
        ]
        .into_iter()
        .any(|code| self.has_cloudkit_code(code))
    }

    fn is_ambiguous_write(&self) -> bool {
        matches!(self, Self::Timeout)
            || self.has_cloudkit_code(CKErrorCode::NetworkFailure)
            || self.has_cloudkit_code(CKErrorCode::ServerResponseLost)
    }

    fn retry_delay(&self, attempt: usize) -> Duration {
        let retry_after = match self {
            Self::CloudKit { retry_after, .. } => *retry_after,
            Self::Write(error) => return error.retry_delay(attempt),
            _ => None,
        };
        retry_after
            .unwrap_or_else(|| Duration::from_millis(500 * (1 << attempt)))
            .min(Duration::from_secs(30))
    }

    fn into_postgate(self) -> PostGateError {
        if self.has_cloudkit_code(CKErrorCode::MissingEntitlement) {
            return PostGateError::InvalidState(
                "CloudKit entitlement is missing; sign PostGate with the configured iCloud container"
                    .into(),
            );
        }
        if self.has_cloudkit_code(CKErrorCode::NotAuthenticated) {
            return PostGateError::InvalidState(
                "No iCloud account is available on this Mac".into(),
            );
        }
        if self.has_cloudkit_code(CKErrorCode::BadContainer) {
            return PostGateError::InvalidState(
                "The PostGate CloudKit container is not provisioned for this build".into(),
            );
        }
        if self.has_cloudkit_code(CKErrorCode::PermissionFailure) {
            return PostGateError::InvalidState(
                "The current iCloud account cannot access the PostGate CloudKit container".into(),
            );
        }
        if self.has_cloudkit_code(CKErrorCode::ServerRejectedRequest) {
            return PostGateError::InvalidState(
                "CloudKit rejected the profile request; verify that the Production schema contains PostGateProfile.payload as an Asset field"
                    .into(),
            );
        }

        match self {
            Self::Conflict(message) | Self::UnknownWriteOutcome(message) => {
                PostGateError::InvalidState(message)
            }
            Self::PayloadTooLarge { size, limit } => PostGateError::InvalidState(format!(
                "CloudKit profile is {} bytes; the maximum supported size is {} bytes",
                size, limit
            )),
            Self::Write(error) => error.into_postgate(),
            error => PostGateError::Storage(format!("CloudKit operation failed: {error}")),
        }
    }
}

impl fmt::Display for NativeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CloudKit {
                domain,
                code,
                message,
                ..
            } => write!(formatter, "{message} ({domain} {code})"),
            Self::Write(error) => error.fmt(formatter),
            Self::Conflict(message)
            | Self::UnknownWriteOutcome(message)
            | Self::Bridge(message) => formatter.write_str(message),
            Self::PayloadTooLarge { size, limit } => {
                write!(formatter, "profile size {size} exceeds limit {limit}")
            }
            Self::Io(error) => error.fmt(formatter),
            Self::Timeout => formatter.write_str("operation timed out"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cloudkit_error(domain: &str, code: CKErrorCode) -> NativeError {
        NativeError::CloudKit {
            domain: domain.into(),
            code: code.0,
            message: "test".into(),
            retry_after: None,
        }
    }

    #[test]
    fn missing_entitlements_are_reported_without_entering_cloudkit() {
        assert!(!entitlement_array_contains(
            "com.alkinum.postgate.missing-entitlement",
            CONTAINER_ID,
        ));
    }

    #[test]
    fn embedded_profile_must_be_in_a_macos_bundle() {
        let root = tempfile::tempdir().expect("temp directory");
        let executable = root.path().join("PostGate.app/Contents/MacOS/postgate");
        let profile = root
            .path()
            .join("PostGate.app/Contents/embedded.provisionprofile");
        std::fs::create_dir_all(executable.parent().expect("executable parent"))
            .expect("bundle directories");
        std::fs::write(&profile, b"profile").expect("profile");

        assert_eq!(
            embedded_profile_path(&executable).as_deref(),
            Some(profile.as_path())
        );
        assert!(embedded_profile_path(root.path()).is_none());
    }

    #[test]
    fn cloudkit_error_codes_require_the_cloudkit_domain() {
        assert!(cloudkit_error("CKErrorDomain", CKErrorCode::UnknownItem).is_unknown_item());
        assert!(!cloudkit_error("NSCocoaErrorDomain", CKErrorCode::UnknownItem).is_unknown_item());
    }

    #[test]
    fn cloudkit_account_errors_have_actionable_messages() {
        let error = cloudkit_error("CKErrorDomain", CKErrorCode::NotAuthenticated).into_postgate();
        assert!(error.to_string().contains("No iCloud account"));
    }

    #[test]
    fn rejected_requests_point_to_the_required_production_schema() {
        let error =
            cloudkit_error("CKErrorDomain", CKErrorCode::ServerRejectedRequest).into_postgate();
        let message = error.to_string();
        assert!(message.contains("Production schema"));
        assert!(message.contains("PostGateProfile.payload"));
    }

    #[test]
    fn retry_after_overrides_exponential_backoff() {
        let error = NativeError::CloudKit {
            domain: "CKErrorDomain".into(),
            code: CKErrorCode::RequestRateLimited.0,
            message: "limited".into(),
            retry_after: Some(Duration::from_secs(7)),
        };
        assert_eq!(error.retry_delay(0), Duration::from_secs(7));
    }

    #[test]
    fn ambiguous_write_errors_are_not_blindly_retried() {
        let network_failure = cloudkit_error("CKErrorDomain", CKErrorCode::NetworkFailure);
        assert!(network_failure.is_retryable_fetch());
        assert!(network_failure.is_ambiguous_write());
        assert!(!network_failure.is_retryable_write());

        let rate_limited = cloudkit_error("CKErrorDomain", CKErrorCode::RequestRateLimited);
        assert!(rate_limited.is_retryable_write());
        assert!(!rate_limited.is_ambiguous_write());
    }

    #[test]
    fn oversized_profiles_are_rejected() {
        assert!(validate_payload_size(MAX_PROFILE_BYTES).is_ok());
        assert!(validate_payload_size(MAX_PROFILE_BYTES + 1).is_err());
    }
}
