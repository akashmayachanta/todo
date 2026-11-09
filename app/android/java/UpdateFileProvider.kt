// Copied into gen/android by scripts/android-native.mjs. Edit it here, not there.
//
// Hands a downloaded app update (cache/updates/tasks.apk, see updates.rs) to
// Android's installer. A subclass of its own, so it can't clash with another
// FileProvider in the manifest.
package dev.tasks.app

import android.content.Context
import androidx.core.content.FileProvider

class UpdateFileProvider : FileProvider() {
  companion object {
    fun authority(context: Context) = "${context.packageName}.updates"
  }
}
