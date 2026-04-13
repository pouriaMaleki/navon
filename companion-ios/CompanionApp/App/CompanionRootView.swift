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
