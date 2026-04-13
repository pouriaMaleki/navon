import SwiftUI

@main
struct ESP32MapCompanionApp: App {
    @StateObject private var appModel = AppModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            CompanionRootView(homeViewModel: HomeViewModel(appModel: appModel))
                .environmentObject(appModel)
                .task {
                    await appModel.consumePendingSharedImports()
                }
                .onOpenURL { _ in
                    Task {
                        await appModel.consumePendingSharedImports()
                    }
                }
                .onChange(of: scenePhase) { _, newValue in
                    guard newValue == .active else { return }
                    Task {
                        await appModel.consumePendingSharedImports()
                    }
                }
        }
    }
}
