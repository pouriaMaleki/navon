import SwiftUI

struct SearchPanelView: View {
    @EnvironmentObject private var searchController: SearchController
    @ObservedObject var viewModel: HomeViewModel
    var isSearchFieldFocused: FocusState<Bool>.Binding

    var body: some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                if searchController.isResolvingUrl {
                    HStack(alignment: .center, spacing: 12) {
                        ProgressView()
                        VStack(alignment: .leading, spacing: 4) {
                            Text(T.string("home.resolvingLink"))
                                .font(.headline)
                            Text(T.string("home.resolvingLinkSubtitle"))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                    }
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                } else if let urlError = searchController.urlResolveError {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(T.string("home.linkFailed"))
                            .font(.headline)
                        Text(urlError)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                } else if searchController.query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    ForEach(Array(viewModel.recentItems.enumerated()), id: \.element.id) { index, item in
                        Button {
                            isSearchFieldFocused.wrappedValue = false
                            viewModel.selectRecent(item)
                        } label: {
                            routeHistoryRow(item)
                        }
                        .buttonStyle(.plain)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                        .accessibilityIdentifier("searchRow-\(index)")
                        .onAppear { searchController.loadMoreRecentsIfNeeded(for: item) }
                        Divider()
                    }
                } else {
                    ForEach(Array(searchController.suggestions.prefix(searchController.visibleSuggestionCount).enumerated()), id: \.element.id) { index, suggestion in
                        Button {
                            isSearchFieldFocused.wrappedValue = false
                            viewModel.selectSuggestion(suggestion)
                        } label: {
                            suggestionRow(suggestion)
                        }
                        .buttonStyle(.plain)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                        .accessibilityIdentifier("searchRow-\(index)")
                        .onAppear { searchController.loadMoreSuggestionsIfNeeded(for: suggestion) }
                        Divider()
                    }
                }
            }
        }
        .frame(maxHeight: 320)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .accessibilityIdentifier("searchPanel")
    }

    private func routeHistoryRow(_ item: RouteHistoryItem) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: item.routePackage == nil ? "clock" : "point.topleft.down.curvedto.point.bottomright.up")
                .foregroundStyle(.teal)
            VStack(alignment: .leading, spacing: 4) {
                Text(item.title)
                    .font(.headline)
                    .foregroundStyle(.primary)
                Text(item.subtitle)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Text(item.sourceLabel)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .contentShape(Rectangle())
    }

    private func suggestionRow(_ suggestion: DestinationSearchResult) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "mappin.and.ellipse")
                .foregroundStyle(.blue)
            VStack(alignment: .leading, spacing: 4) {
                Text(suggestion.title)
                    .font(.headline)
                    .foregroundStyle(.primary)
                if !suggestion.subtitle.isEmpty {
                    Text(suggestion.subtitle)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
            }
            Spacer()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .contentShape(Rectangle())
    }
}
