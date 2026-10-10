import Foundation

struct ConnectionReconnectSchedule: Equatable {
    let delayMilliseconds: UInt64
}

protocol ConnectionReconnectCancellable: AnyObject {
    func cancel()
}

protocol ConnectionReconnectScheduling: AnyObject {
    func schedule(after delayMilliseconds: UInt64, operation: @escaping () -> Void)
        -> any ConnectionReconnectCancellable
}

final class ConnectionReconnectController {
    private let scheduler: any ConnectionReconnectScheduling
    private var pending: (any ConnectionReconnectCancellable)?

    init(scheduler: any ConnectionReconnectScheduling) {
        self.scheduler = scheduler
    }

    func schedule(after delayMilliseconds: UInt64, operation: @escaping () -> Void) -> ConnectionReconnectSchedule {
        pending?.cancel()
        pending = scheduler.schedule(after: delayMilliseconds, operation: operation)
        return ConnectionReconnectSchedule(delayMilliseconds: delayMilliseconds)
    }

    func cancel() {
        pending?.cancel()
        pending = nil
    }
}

private final class DispatchReconnectCancellation: ConnectionReconnectCancellable {
    private let workItem: DispatchWorkItem

    init(workItem: DispatchWorkItem) {
        self.workItem = workItem
    }

    func cancel() {
        workItem.cancel()
    }
}

final class DispatchQueueReconnectScheduler: ConnectionReconnectScheduling {
    private let queue: DispatchQueue

    init(queue: DispatchQueue) {
        self.queue = queue
    }

    func schedule(after delayMilliseconds: UInt64, operation: @escaping () -> Void)
        -> any ConnectionReconnectCancellable
    {
        let workItem = DispatchWorkItem(block: operation)
        queue.asyncAfter(
            deadline: .now() + .milliseconds(Int(delayMilliseconds)),
            execute: workItem
        )
        return DispatchReconnectCancellation(workItem: workItem)
    }
}
