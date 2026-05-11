package app.navon.bike.flows

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import app.navon.bike.MainActivity
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Plan flow #46 — long-press the map to drop a destination pin (spec line 76).
 *
 * After a long-press, the where-to input must be populated with a resolved
 * address or coordinate. Expected RED on Android until the long-press handler
 * is wired.
 */
@RunWith(AndroidJUnit4::class)
class LongPressDestinationTest {
    @get:Rule val composeRule = createAndroidComposeRule<MainActivity>()

    @Test
    fun long_press_on_map_populates_where_to_input() {
        composeRule.onNodeWithTag("mapSurface").performTouchInput { longClick() }
        // The input's EditableText should be set to something; if nothing has
        // changed it is either the placeholder or empty. Reject both.
        composeRule.onNodeWithTag("whereToInput").assert(
            SemanticsMatcher("EditableText is non-empty and not placeholder") { node ->
                val editable = node.config.getOrNull(SemanticsProperties.EditableText)
                    ?.text
                    ?.trim()
                    .orEmpty()
                editable.isNotEmpty() && editable != "Where to?"
            }
        )
    }
}
