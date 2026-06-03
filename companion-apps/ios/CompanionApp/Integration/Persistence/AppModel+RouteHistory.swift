import Foundation

extension AppModel {
    func activateRouteHistoryItem(_ item: RouteHistoryItem, startImmediately: Bool = false) {
        Task {
            await routeHistoryService.resetCurrentRouteForHistoryActivation()
            await applyRouteHistoryPreview(item)
            homePreviewRequestID = UUID()
            if startImmediately {
                homeStartRequestID = UUID()
            }
        }
    }
}
