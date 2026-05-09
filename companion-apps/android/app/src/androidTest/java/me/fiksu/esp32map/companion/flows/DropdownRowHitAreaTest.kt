package me.fiksu.esp32map.companion.flows

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import me.fiksu.esp32map.companion.MainActivity
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Plan flow #32 — full-row tap target on the where-to dropdown.
 *
 * Regression-lock for a bug where the dropdown row's hit area collapsed to
 * the text width, so taps in the padding missed. This test deliberately
 * clicks near the LEADING EDGE of the row (5 px in) rather than the centre
 * so a regression that shrinks the hit area is caught.
 */
@RunWith(AndroidJUnit4::class)
class DropdownRowHitAreaTest {
    @get:Rule val composeRule = createAndroidComposeRule<MainActivity>()

    @Test
    fun tap_near_leading_edge_selects_item() {
        composeRule.onNodeWithTag("whereToInput").performTextInput("hel")
        composeRule.onNodeWithTag("searchRow-0").assertExists()
        composeRule.onNodeWithTag("searchRow-0").performTouchInput {
            // 5 px from the leading edge, vertically centred. Any row that
            // relies on text-width padding instead of `fillMaxWidth` will
            // miss this tap.
            click(Offset(5f, height / 2f))
        }
        composeRule.onNodeWithTag("searchRow-0").assertDoesNotExist()
    }
}
