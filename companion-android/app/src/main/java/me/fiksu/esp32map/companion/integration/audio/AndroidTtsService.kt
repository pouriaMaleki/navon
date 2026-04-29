package me.fiksu.esp32map.companion.integration.audio

import android.content.Context
import android.speech.tts.TextToSpeech
import java.util.Locale

/**
 * Thin wrapper over Android's [TextToSpeech] used to speak audio cues during
 * routing. Cancels any in-flight utterance when a new cue arrives so the
 * latest cue is what the rider hears (turn-by-turn cues need recency, not
 * completeness).
 */
interface TtsPort {
    fun speak(text: String)
    fun shutdown()
}

class AndroidTtsService(context: Context) : TtsPort {
    private var tts: TextToSpeech? = null
    private var ready = false

    init {
        tts = TextToSpeech(context.applicationContext) { status ->
            if (status == TextToSpeech.SUCCESS) {
                tts?.language = Locale.getDefault()
                ready = true
            }
        }
    }

    override fun speak(text: String) {
        if (!ready) return
        // QUEUE_FLUSH cancels any in-flight utterance and replaces with the new one.
        tts?.speak(text, TextToSpeech.QUEUE_FLUSH, null, "esp32-cue")
    }

    override fun shutdown() {
        tts?.stop()
        tts?.shutdown()
        tts = null
    }
}
