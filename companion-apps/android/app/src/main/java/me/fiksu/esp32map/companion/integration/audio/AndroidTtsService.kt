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
    /** True when Android has at least one TTS voice installed for the
     *  primary language tag of `locale`. Used by the cue dispatcher to
     *  fall back to English audio when the rider's language isn't
     *  installed. Returns `true` while the engine is still initialising
     *  (no false alarms during the first second of app launch). */
    fun hasVoice(forLocale: String): Boolean
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

    /** Returns `true` while the engine is still initialising (engine is
     *  null) so the UI doesn't false-alarm during the first second of
     *  app launch. After init, queries [TextToSpeech.isLanguageAvailable]
     *  and returns true for `LANG_AVAILABLE` (0) and the more-specific
     *  `LANG_COUNTRY_AVAILABLE` (1) / `LANG_COUNTRY_VAR_AVAILABLE` (2)
     *  results. Treats `LANG_MISSING_DATA` (-1) and `LANG_NOT_SUPPORTED`
     *  (-2) as "no voice". */
    override fun hasVoice(forLocale: String): Boolean {
        val engine = tts ?: return true
        if (!ready) return true
        val locale = Locale.forLanguageTag(forLocale)
        return engine.isLanguageAvailable(locale) >= TextToSpeech.LANG_AVAILABLE
    }

    override fun shutdown() {
        tts?.stop()
        tts?.shutdown()
        tts = null
    }
}
