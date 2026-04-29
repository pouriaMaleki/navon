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
                    switch newValue {
                    case .active:
                        appModel.handleApplicationLifecycleEnteredForeground()
                        Task { await appModel.consumePendingSharedImports() }
                    case .background:
                        appModel.handleApplicationLifecycleEnteredBackground()
                    case .inactive:
                        break
                    @unknown default:
                        break
                    }
                }
        }
    }
}
