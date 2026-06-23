import SwiftUI

@main
struct NavonApp: App {
    @StateObject private var appModel = AppModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            CompanionRootView(homeViewModel: HomeViewModel(appModel: appModel))
                .environmentObject(appModel)
                .environmentObject(appModel.routeHistoryService)
                .environmentObject(appModel.shareImportService)
                // RTL support. SwiftUI flips horizontal stacks, paddings,
                // chevrons, etc. once `layoutDirection` is set on the
                // root view. Keying off `appModel.settings.language`
                // (an @Published property) makes the whole tree re-lay
                // when the rider switches between an LTR locale (English,
                // Finnish, …) and an RTL one (Arabic, Persian, Urdu).
                .environment(
                    \.layoutDirection,
                    T.resolveLocale(appModel.settings.language).isRtl ? .rightToLeft : .leftToRight
                )
                .task {
                    await appModel.shareImportService.consumePendingSharedImports()
                }
                .onOpenURL { url in
                    Task {
                        if url.isFileURL {
                            await appModel.importGpxFile(from: url)
                        } else {
                            await appModel.shareImportService.consumePendingSharedImports()
                        }
                    }
                }
                .onChange(of: scenePhase) { _, newValue in
                    switch newValue {
                    case .active:
                        appModel.handleApplicationLifecycleEnteredForeground()
                        Task { await appModel.shareImportService.consumePendingSharedImports() }
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
