package ai.tnn.localai

import android.os.Bundle
import android.content.Intent
import android.net.Uri
import java.io.File
import androidx.annotation.Keep
import androidx.core.content.FileProvider
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // Keep the WebView above system bars/cutouts and the on-screen keyboard.
    // Consuming these insets avoids applying the same padding again in CSS.
    ViewCompat.setOnApplyWindowInsetsListener(window.decorView) { view, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
      )
      val keyboard = insets.getInsets(WindowInsetsCompat.Type.ime())
      view.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, keyboard.bottom))
      WindowInsetsCompat.CONSUMED
    }
    ViewCompat.requestApplyInsets(window.decorView)
  }

  @Keep
  fun openModelDocument(value: String): Int {
    val uri = Uri.parse(value)
    require(uri.scheme == "content")
    return contentResolver.openFileDescriptor(uri, "r")?.use { it.detachFd() }
      ?: throw IllegalArgumentException("The selected document is not readable")
  }

  @Keep
  fun shareExport(value: String) {
    val file = File(value).canonicalFile
    val exports = File(filesDir, "exports").canonicalFile
    require(file.parentFile == exports && file.isFile)
    val uri = FileProvider.getUriForFile(this, "$packageName.fileprovider", file)
    val type = when (file.extension) {
      "json" -> "application/json"
      "zip" -> "application/zip"
      else -> "text/plain"
    }
    runOnUiThread {
      val intent = Intent(Intent.ACTION_SEND).apply {
        this.type = type
        putExtra(Intent.EXTRA_STREAM, uri)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      }
      startActivity(Intent.createChooser(intent, null))
    }
  }
}
