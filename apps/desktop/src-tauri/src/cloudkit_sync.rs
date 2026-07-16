use crate::error::{PostGateError, Result};
use block2::RcBlock;
use core_foundation_sys::array::{
    CFArrayGetCount, CFArrayGetTypeID, CFArrayGetValueAtIndex, CFArrayRef,
};
use core_foundation_sys::base::{CFGetTypeID, CFRelease, CFTypeRef};
use core_foundation_sys::error::CFErrorRef;
use core_foundation_sys::string::{kCFStringEncodingUTF8, CFStringCreateWithCString, CFStringRef};
use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::AnyThread;
use objc2_cloud_kit::{
    CKAsset, CKContainer, CKDatabase, CKErrorCode, CKRecord, CKRecordID, CKRecordValue,
};
use objc2_foundation::{NSError, NSString, NSURL};
use std::ffi::{c_void, CString};
use std::fmt;
use std::io::Write;
use std::ptr;
use std::sync::{mpsc, Arc};
use std::time::Duration;

pub const CONTAINER_ID: &str = "iCloud.com.alkinum.postgate";
pub const LOCATION: &str = "cloudkit://iCloud.com.alkinum.postgate/private/profile-v1";

const RECORD_TYPE: &str = "PostGateProfile";
const RECORD_NAME: &str = "profile-v1";
const PAYLOAD_FIELD: &str = "payload";
const OPERATION_TIMEOUT: Duration = Duration::from_secs(45);
const CONTAINERS_ENTITLEMENT: &str = "com.apple.developer.icloud-container-identifiers";
const SERVICES_ENTITLEMENT: &str = "com.apple.developer.icloud-services";

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
}

pub async fn exists() -> Result<bool> {
    ensure_available()?;
    run_blocking(fetch_optional)
        .await
        .map(|profile| profile.is_some())
}

pub async fn pull() -> Result<RemoteProfile> {
    ensure_available()?;
    run_blocking(fetch_optional)
        .await?
        .ok_or_else(|| PostGateError::NotFound("No CloudKit profile has been uploaded".into()))
}

pub async fn push(payload: Vec<u8>, expected_change_tag: Option<String>) -> Result<String> {
    ensure_available()?;
    run_blocking(move || push_blocking(payload, expected_change_tag)).await
}

fn ensure_available() -> Result<()> {
    if is_available() {
        Ok(())
    } else {
        Err(PostGateError::InvalidState(
            "CloudKit is unavailable because this build is not signed with the PostGate iCloud entitlements"
                .into(),
        ))
    }
}

fn entitlement_array_contains(entitlement: &str, expected: &str) -> bool {
    let Ok(entitlement) = CString::new(entitlement) else {
        return false;
    };
    let Ok(expected) = CString::new(expected) else {
        return false;
    };

    unsafe {
        let task = SecTaskCreateFromSelf(ptr::null());
        if task.is_null() {
            return false;
        }
        let entitlement =
            CFStringCreateWithCString(ptr::null(), entitlement.as_ptr(), kCFStringEncodingUTF8);
        if entitlement.is_null() {
            CFRelease(task as CFTypeRef);
            return false;
        }
        let value = SecTaskCopyValueForEntitlement(task, entitlement, ptr::null_mut());
        CFRelease(entitlement as CFTypeRef);
        CFRelease(task as CFTypeRef);
        if value.is_null() || CFGetTypeID(value) != CFArrayGetTypeID() {
            if !value.is_null() {
                CFRelease(value);
            }
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

fn fetch_optional() -> std::result::Result<Option<RemoteProfile>, NativeError> {
    autoreleasepool(|_| {
        let database = private_database()?;
        let record_id = record_id();
        let (sender, receiver) = mpsc::channel();
        let completion = RcBlock::new(move |record: *mut CKRecord, error: *mut NSError| {
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
                let _ = sender.send(result);
            });
        });

        unsafe {
            database.fetchRecordWithID_completionHandler(&record_id, &completion);
        }

        receiver
            .recv_timeout(OPERATION_TIMEOUT)
            .map_err(|_| NativeError::Timeout)?
    })
}

fn push_blocking(
    payload: Vec<u8>,
    expected_change_tag: Option<String>,
) -> std::result::Result<String, NativeError> {
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

    autoreleasepool(|_| {
        let database = private_database()?;
        let callback_database = database.clone();
        let record_id = record_id();
        let callback_record_id = record_id.clone();
        let (sender, receiver) = mpsc::channel();
        let completion = RcBlock::new(move |record: *mut CKRecord, error: *mut NSError| {
            autoreleasepool(|_| match native_error(error) {
                Some(error) if error.is_unknown_item() => {
                    let record = unsafe {
                        CKRecord::initWithRecordType_recordID(
                            CKRecord::alloc(),
                            &NSString::from_str(RECORD_TYPE),
                            &callback_record_id,
                        )
                    };
                    save_record(
                        &callback_database,
                        &record,
                        asset_path.clone(),
                        sender.clone(),
                    );
                }
                Some(error) => {
                    let _ = sender.send(Err(error));
                }
                None => {
                    let Some(record) = (unsafe { record.as_ref() }) else {
                        let _ = sender.send(Err(NativeError::Bridge(
                            "CloudKit returned an empty record".into(),
                        )));
                        return;
                    };
                    let remote_change_tag =
                        unsafe { record.recordChangeTag() }.map(|value| value.to_string());
                    if remote_change_tag.as_deref() != expected_change_tag.as_deref() {
                        let _ = sender.send(Err(NativeError::Conflict(
                            "A newer CloudKit profile exists; pull it before pushing local changes"
                                .into(),
                        )));
                        return;
                    }
                    save_record(
                        &callback_database,
                        record,
                        asset_path.clone(),
                        sender.clone(),
                    );
                }
            });
        });

        unsafe {
            database.fetchRecordWithID_completionHandler(&record_id, &completion);
        }

        receiver
            .recv_timeout(OPERATION_TIMEOUT)
            .map_err(|_| NativeError::Timeout)?
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
                Some(error) => Err(error),
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
    let payload = std::fs::read(path).map_err(NativeError::Io)?;
    let change_tag = unsafe { record.recordChangeTag() }
        .map(|tag| tag.to_string())
        .ok_or_else(|| NativeError::Bridge("CloudKit profile has no change tag".into()))?;

    Ok(RemoteProfile {
        payload,
        change_tag,
    })
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
    Some(NativeError::CloudKit {
        domain: error.domain().to_string(),
        code: error.code(),
        message: error.localizedDescription().to_string(),
    })
}

#[derive(Debug)]
enum NativeError {
    CloudKit {
        domain: String,
        code: isize,
        message: String,
    },
    Conflict(String),
    Bridge(String),
    Io(std::io::Error),
    Timeout,
}

impl NativeError {
    fn has_cloudkit_code(&self, expected: CKErrorCode) -> bool {
        matches!(self, Self::CloudKit { domain, code, .. } if domain == "CKErrorDomain" && *code == expected.0)
    }

    fn is_unknown_item(&self) -> bool {
        self.has_cloudkit_code(CKErrorCode::UnknownItem)
    }

    fn is_server_record_changed(&self) -> bool {
        self.has_cloudkit_code(CKErrorCode::ServerRecordChanged)
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

        match self {
            Self::Conflict(message) => PostGateError::InvalidState(message),
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
            } => write!(formatter, "{message} ({domain} {code})"),
            Self::Conflict(message) | Self::Bridge(message) => formatter.write_str(message),
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
    fn cloudkit_error_codes_require_the_cloudkit_domain() {
        assert!(cloudkit_error("CKErrorDomain", CKErrorCode::UnknownItem).is_unknown_item());
        assert!(!cloudkit_error("NSCocoaErrorDomain", CKErrorCode::UnknownItem).is_unknown_item());
    }

    #[test]
    fn cloudkit_account_errors_have_actionable_messages() {
        let error = cloudkit_error("CKErrorDomain", CKErrorCode::NotAuthenticated).into_postgate();
        assert!(error.to_string().contains("No iCloud account"));
    }
}
