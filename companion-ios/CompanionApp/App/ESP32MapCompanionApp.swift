import SwiftUI

@main
struct ESP32MapCompanionApp: App {
    @StateObject private var appModel = AppModel()

    var body: some Scene {
        WindowGroup {
            CompanionRootView(homeViewModel: HomeViewModel(appModel: appModel))
                .environmentObject(appModel)
        }
    }
}
