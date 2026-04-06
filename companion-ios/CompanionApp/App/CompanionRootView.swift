import SwiftUI

struct CompanionRootView: View {
    @EnvironmentObject private var appModel: AppModel
    @StateObject private var homeViewModel: HomeViewModel
    @State private var showingSettings = false

    init(homeViewModel: HomeViewModel) {
        _homeViewModel = StateObject(wrappedValue: homeViewModel)
    }

    var body: some View {
        NavigationStack {
            CompanionHomeView(viewModel: homeViewModel)
                .toolbar {
                    ToolbarItem(placement: .topBarTrailing) {
                        Button {
                            showingSettings = true
                        } label: {
                            Image(systemName: "gearshape.fill")
                        }
                        .accessibilityLabel("Settings")
                    }
                }
        }
        .fullScreenCover(isPresented: $showingSettings) {
            SettingsHubView()
                .environmentObject(appModel)
        }
    }
}
