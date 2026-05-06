import WidgetKit
import SwiftUI

@main
struct CompanionLiveActivityBundle: WidgetBundle {
    var body: some Widget {
        if #available(iOS 16.2, *) {
            RouteGuidanceLiveActivity()
        }
    }
}
