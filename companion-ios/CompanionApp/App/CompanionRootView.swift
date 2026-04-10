import SwiftUI

struct CompanionRootView: View {
    @EnvironmentObject private var appModel: AppModel
    @StateObject private var homeViewModel: HomeViewModel
    @State private var showingSettings = false

    init(homeViewModel: HomeViewModel) {
        _homeViewModel = StateObject(wrappedValue: homeViewModel)
    }

    var body: some View {
        CompanionHomeView(
            viewModel: homeViewModel,
            onOpenSettings: { showingSettings = true }
        )
        .onChange(of: appModel.homePreviewRequestID) { _, _ in
            showingSettings = false
            homeViewModel.syncQueryFromCurrentPreview()
        }
        .onChange(of: appModel.homeStartRequestID) { _, _ in
            showingSettings = false
            Task { await homeViewModel.startSelectedRoute() }
        }
        .fullScreenCover(isPresented: $showingSettings) {
            SettingsHubView()
                .environmentObject(appModel)
        }
    }
}
