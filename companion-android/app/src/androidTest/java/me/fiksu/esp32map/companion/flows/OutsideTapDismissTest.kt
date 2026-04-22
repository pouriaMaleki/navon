package me.fiksu.esp32map.companion.flows

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import me.fiksu.esp32map.companion.MainActivity
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Plan flow #33 — tapping outside the search panel dismisses the dropdown.
 */
@RunWith(AndroidJUnit4::class)
class OutsideTapDismissTest {
    @get:Rule val composeRule = createAndroidComposeRule<MainActivity>()

    @Test
    fun outside_tap_dismisses_search_panel() {
        composeRule.onNodeWithTag("whereToInput").performTextInput("hel")
        composeRule.onNodeWithTag("searchPanel").assertExists()
        composeRule.onNodeWithTag("mapSurface").performClick()
        composeRule.onNodeWithTag("searchPanel").assertDoesNotExist()
    }
}
