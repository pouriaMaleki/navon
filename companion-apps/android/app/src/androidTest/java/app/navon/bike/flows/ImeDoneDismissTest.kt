package app.navon.bike.flows

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performImeAction
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import app.navon.bike.MainActivity
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Plan flow #34 (IME Done variant) — Done key dismisses the search panel.
 */
@RunWith(AndroidJUnit4::class)
class ImeDoneDismissTest {
    @get:Rule val composeRule = createAndroidComposeRule<MainActivity>()

    @Test
    fun ime_done_dismisses_search_panel() {
        composeRule.onNodeWithTag("whereToInput").performTextInput("hel")
        composeRule.onNodeWithTag("searchPanel").assertExists()
        composeRule.onNodeWithTag("whereToInput").performImeAction()
        composeRule.onNodeWithTag("searchPanel").assertDoesNotExist()
    }
}
