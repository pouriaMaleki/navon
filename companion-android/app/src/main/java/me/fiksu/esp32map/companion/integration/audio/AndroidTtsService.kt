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
    fun setLanguage(bcp47: String)
    fun shutdown()
}

class AndroidTtsService(context: Context) : TtsPort {
    private var tts: TextToSpeech? = null
    private var ready = false
    private var pendingLocale: Locale = Locale.getDefault()

    init {
        tts = TextToSpeech(context.applicationContext) { status ->
            if (status == TextToSpeech.SUCCESS) {
                tts?.language = pendingLocale
                ready = true
            }
        }
    }

    override fun speak(text: String) {
        if (!ready) return
        // QUEUE_FLUSH cancels any in-flight utterance and replaces with the new one.
        tts?.speak(text, TextToSpeech.QUEUE_FLUSH, null, "esp32-cue")
    }

    /** Update the language used for subsequent utterances. If the engine
     *  hasn't initialised yet, the locale is queued and applied when it
     *  becomes ready. */
    override fun setLanguage(bcp47: String) {
        val locale = Locale.forLanguageTag(bcp47)
        pendingLocale = locale
        if (ready) {
            tts?.language = locale
        }
    }

    override fun shutdown() {
        tts?.stop()
        tts?.shutdown()
        tts = null
    }
}
