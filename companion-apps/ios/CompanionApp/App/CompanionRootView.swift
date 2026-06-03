import SwiftUI

struct CompanionRootView: View {
    @EnvironmentObject private var appModel: AppModel
    @StateObject private var homeViewModel: HomeViewModel
    @State private var showingSettings = false

    /// Two-layer autoclosure deferral: the caller writes
    /// `CompanionRootView(homeViewModel: HomeViewModel(appModel: appModel))`
    /// and Swift captures `HomeViewModel(appModel: appModel)` as the body of
    /// the `homeViewModel` parameter's autoclosure. The expression
    /// `homeViewModel()` below is then captured AS-IS into `StateObject`'s
    /// own `@autoclosure @escaping` `wrappedValue` parameter — never
    /// evaluated to a value here. SwiftUI only invokes that chain on the
    /// view's first `@StateObject` initialisation; on subsequent re-inits
    /// (every `WindowGroup` body re-eval) the stored value is kept and
    /// neither closure fires, so the `HomeViewModel` is constructed once.
    ///
    /// If you "simplify" this to `init(homeViewModel: HomeViewModel)`, the
    /// caller's `HomeViewModel(...)` evaluates eagerly on every re-eval —
    /// `@StateObject` still discards the duplicates, but you pay the
    /// allocation cost each time. Keep the autoclosure.
    init(homeViewModel: @autoclosure @escaping () -> HomeViewModel) {
        _homeViewModel = StateObject(wrappedValue: homeViewModel())
    }

    var body: some View {
        CompanionHomeView(
            viewModel: homeViewModel,
            onOpenSettings: { showingSettings = true }
        )
        .environmentObject(homeViewModel.searchController)
        .task(id: appModel.homePreviewRequestID) {
            showingSettings = false
            await homeViewModel.revealImportedPreview()
        }
        .task(id: appModel.homeStartRequestID) {
            showingSettings = false
            await homeViewModel.startSelectedRoute()
        }
        .fullScreenCover(isPresented: $showingSettings) {
            SettingsHubView()
                .environmentObject(appModel)
        }
    }
}
