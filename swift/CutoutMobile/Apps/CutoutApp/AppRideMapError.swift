import CutoutMobile

func appRideMapError(_ error: Error) -> MobileRideMapError {
    if let error = error as? MobileRideMapError { return error }
    return .storageError(String(describing: error))
}
