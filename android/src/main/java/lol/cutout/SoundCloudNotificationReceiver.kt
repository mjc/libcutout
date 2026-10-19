package lol.cutout

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import uniffi.cutout_mobile_ffi.MobileSoundCloudCommand

/** An explicit notification tap reaches the app-owned player before requesting service lifetime. */
class SoundCloudNotificationReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val command = MobileSoundCloudCommand.entries.firstOrNull { it.name == intent.getStringExtra(COMMAND) } ?: return
        (context.applicationContext as CutoutApplication).existingSoundCloudMusic?.command(command)
    }

    companion object {
        private const val COMMAND = "soundcloud-command"
        fun pendingIntent(context: Context, command: MobileSoundCloudCommand): PendingIntent =
            PendingIntent.getBroadcast(context, command.ordinal,
                Intent(context, SoundCloudNotificationReceiver::class.java).putExtra(COMMAND, command.name),
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    }
}
