//! Android API adapters only; model validation and storage stay in tnn-core.

use std::fs::File;
use std::os::fd::FromRawFd;
use std::path::{Path, PathBuf};

use jni::objects::{JObject, JString, JValue};

fn with_activity<T>(
    f: impl FnOnce(&mut jni::JNIEnv<'_>, &JObject<'_>) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let context = tauri::tao::platform::android::prelude::main_android_context()
        .ok_or_else(|| anyhow::anyhow!("Android activity is not available"))?;
    // SAFETY: Tao owns these VM/activity references for the app's lifetime.
    let vm = unsafe { jni::JavaVM::from_raw(context.java_vm.cast()) }?;
    let mut env = vm.attach_current_thread()?;
    let activity = unsafe { JObject::from_raw(context.context_jobject.cast()) };
    let result = env.with_local_frame(16, |env| f(env, &activity));
    // A provider may revoke permission or fail to open a document. Clear the
    // exception so a recoverable picker failure cannot poison later JNI calls.
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    result
}

/// Android 10+ only permits executing the installed, read-only APK payload.
pub fn native_library_dir() -> anyhow::Result<PathBuf> {
    with_activity(|env, activity| {
        let info = env
            .call_method(
                activity,
                "getApplicationInfo",
                "()Landroid/content/pm/ApplicationInfo;",
                &[],
            )?
            .l()?;
        let dir = JString::from(
            env.get_field(&info, "nativeLibraryDir", "Ljava/lang/String;")?
                .l()?,
        );
        let dir: String = env.get_string(&dir)?.into();
        Ok(PathBuf::from(dir))
    })
}

pub fn open_model_document(uri: &str) -> anyhow::Result<File> {
    anyhow::ensure!(
        uri.starts_with("content://") && uri.len() <= 8192,
        "invalid document URI"
    );
    with_activity(|env, activity| {
        let uri = env.new_string(uri)?;
        let fd = env
            .call_method(
                activity,
                "openModelDocument",
                "(Ljava/lang/String;)I",
                &[JValue::Object(uri.as_ref())],
            )?
            .i()?;
        anyhow::ensure!(fd >= 0, "document provider did not return a readable file");
        // SAFETY: Kotlin detachFd transfers sole ownership to Rust.
        Ok(unsafe { File::from_raw_fd(fd) })
    })
}

pub fn share_export(path: &Path) -> anyhow::Result<()> {
    with_activity(|env, activity| {
        let path = env.new_string(path.to_string_lossy())?;
        env.call_method(
            activity,
            "shareExport",
            "(Ljava/lang/String;)V",
            &[JValue::Object(path.as_ref())],
        )?;
        Ok(())
    })
}
