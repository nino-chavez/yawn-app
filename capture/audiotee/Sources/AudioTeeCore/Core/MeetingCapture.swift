import Darwin
import Foundation

public enum MeetingCaptureLeg: String, Codable, CaseIterable, Sendable {
  case mic
  case system
}

public struct MeetingCaptureFault: Error, Codable, Equatable, Sendable {
  public let code: String
  public let leg: MeetingCaptureLeg?
  public let detail: String

  public init(code: String, leg: MeetingCaptureLeg? = nil, detail: String) {
    self.code = code
    self.leg = leg
    self.detail = detail
  }
}

public struct MeetingCaptureReceipt: Codable, Equatable, Sendable {
  public let micSamples: Int
  public let systemSamples: Int

  public init(micSamples: Int, systemSamples: Int) {
    self.micSamples = micSamples
    self.systemSamples = systemSamples
  }
}

public enum MeetingCaptureUpdate: Equatable, Sendable {
  case recording
  /// Both sources have been released and nothing is reaching the WAV pair.
  case suspended
  /// Both sources are acquiring again and audio is reaching the same WAV pair.
  case resumed
  case finalized(MeetingCaptureReceipt)
  case failed(MeetingCaptureFault)
  case interrupted
}

/// One normalized PCM producer. Implementations must emit mono, 16 kHz,
/// signed 16-bit little-endian frames and must not open hardware before start.
public protocol MeetingAudioSource: AnyObject {
  var leg: MeetingCaptureLeg { get }

  func start(
    onPCM: @escaping @Sendable (Data) -> Void,
    onFailure: @escaping @Sendable (MeetingCaptureFault) -> Void
  ) throws

  func stop()
}

public enum MeetingCaptureState: Equatable, Sendable {
  /// Before `activate`. The helper starts here so no hardware is opened until
  /// the application has committed its ownership receipt.
  case paused
  case arming
  case recording
  /// Releasing both sources for an operator pause. Audio is refused from every
  /// leg here, including the remainder a source may hand back from `stop`.
  case suspending
  /// Held open mid-take with no source acquiring. Distinct from `paused`, which
  /// is the pre-start state and is the only state `activate` accepts.
  case suspended
  /// Reacquiring both sources into the WAV pair opened at arming.
  case resuming
  case stopping
  case terminal
}

/// Coordinates two independently-clocked normalized sources without assigning
/// capture-health meaning to their bytes. The first valid block from each leg
/// establishes readiness and is discarded. Files are opened only after both
/// readiness signals have arrived.
public final class MeetingCaptureCoordinator: @unchecked Sendable {
  private let mic: MeetingAudioSource
  private let system: MeetingAudioSource
  private let directoryFD: Int32
  private let maxPendingBytes: Int
  /// A source has this long after activation or resume to deliver a normalized
  /// PCM block. Ten seconds tolerates ordinary device wake-up without leaving a
  /// take Recording forever when one source silently stops calling back.
  private let stallGraceNanoseconds: UInt64
  /// The system tap has no established contract that idle output delivers
  /// nonempty zero PCM. Keep its watchdog opt-in until native evidence proves
  /// that contract; microphone delivery remains protected by default.
  private let monitorsSystemAudioStall: Bool
  private let onUpdate: @Sendable (MeetingCaptureUpdate) -> Void
  private let controlQueue = DispatchQueue(label: "local-meeting-notes.capture-control")
  private let lock = NSLock()

  private var _state: MeetingCaptureState = .paused
  private var readyLegs: Set<MeetingCaptureLeg> = []
  private var stopDrainingLegs: Set<MeetingCaptureLeg> = []
  private var lastBlockDelivery: [MeetingCaptureLeg: UInt64] = [:]
  private var stallWatchdog: DispatchSourceTimer?
  private var watchdogEpoch: UInt64?
  private var acquisitionEpoch: UInt64 = 0
  private var storedFault: MeetingCaptureFault?
  private var pair: PrivateWAVPair?

  public init(
    directoryFD: Int32,
    mic: MeetingAudioSource,
    system: MeetingAudioSource,
    maxPendingBytes: Int = 16_000 * 2 * 5,
    stallGraceSeconds: TimeInterval = 10,
    monitorSystemAudioStall: Bool = false,
    onUpdate: @escaping @Sendable (MeetingCaptureUpdate) -> Void
  ) throws {
    guard mic.leg == .mic, system.leg == .system else {
      throw MeetingCaptureFault(
        code: "source_leg_mismatch", detail: "capture sources do not own the expected legs")
    }
    guard maxPendingBytes > 0 else {
      throw MeetingCaptureFault(
        code: "invalid_queue_limit", detail: "capture queue limit must be positive")
    }
    guard
      stallGraceSeconds.isFinite,
      stallGraceSeconds > 0,
      stallGraceSeconds <= Double(Int.max) / 1_000_000_000
    else {
      throw MeetingCaptureFault(
        code: "invalid_stall_grace", detail: "capture audio-stall grace must be positive")
    }
    guard fcntl(directoryFD, F_GETFD) != -1 else {
      throw MeetingCaptureFault(
        code: "capture_directory_unavailable", detail: "capture directory descriptor is invalid")
    }
    var metadata = stat()
    guard fstat(directoryFD, &metadata) == 0, (metadata.st_mode & S_IFMT) == S_IFDIR else {
      throw MeetingCaptureFault(
        code: "capture_directory_unavailable", detail: "capture descriptor is not a directory")
    }
    guard metadata.st_mode & 0o777 == 0o700 else {
      throw MeetingCaptureFault(
        code: "capture_directory_mode", detail: "capture directory must have mode 0700")
    }

    self.directoryFD = directoryFD
    self.mic = mic
    self.system = system
    self.maxPendingBytes = maxPendingBytes
    self.stallGraceNanoseconds = UInt64(stallGraceSeconds * 1_000_000_000)
    self.monitorsSystemAudioStall = monitorSystemAudioStall
    self.onUpdate = onUpdate
  }

  public var state: MeetingCaptureState {
    lock.withLock { _state }
  }

  /// Begins hardware acquisition asynchronously so the caller can continue to
  /// watch the inherited parent-liveness descriptor while permissions settle.
  public func activate() {
    controlQueue.async { [self] in
      guard transition(from: .paused, to: .arming) else {
        fail(
          MeetingCaptureFault(
            code: "invalid_start", detail: "capture start is valid only while paused"))
        return
      }
      let epoch = beginAcquisition()
      do {
        try start(source: mic, epoch: epoch)
        try start(source: system, epoch: epoch)
      } catch let fault as MeetingCaptureFault {
        fail(fault)
      } catch {
        fail(
          MeetingCaptureFault(
            code: "source_start_failed", detail: String(describing: error)))
      }
    }
  }

  /// Releases the microphone and the system tap without ending the take.
  ///
  /// This is a real hardware release, not a discard filter: the engine stops,
  /// the tap is torn down, and no audio is acquired for the span. The WAV pair
  /// stays open, so what was recorded on either side of the gap remains one
  /// continuous file and the gap itself occupies no samples.
  @discardableResult
  public func suspend() -> Bool {
    controlQueue.sync {
      guard transition(from: .recording, to: .suspending) else { return false }
      // A source may hand back its buffered remainder synchronously from
      // `stop`. A pause is an instruction about the present, so that remainder
      // is dropped: `receive` refuses every leg while the state is suspending.
      cancelStallWatchdog()
      invalidateAcquisition()
      mic.stop()
      system.stop()
      readyLegs = []
      lock.withLock { _state = .suspended }
      onUpdate(.suspended)
      return true
    }
  }

  /// Reacquires both sources into the WAV pair opened at arming.
  ///
  /// The first block from each leg is discarded exactly as it is at arming, so
  /// the resumed audio begins at a block boundary and the caller learns that
  /// capture is live again only once both legs are actually producing.
  public func resume() {
    controlQueue.async { [self] in
      guard transition(from: .suspended, to: .resuming) else {
        fail(
          MeetingCaptureFault(
            code: "invalid_resume", detail: "capture resume is valid only while suspended"))
        return
      }
      let epoch = beginAcquisition()
      do {
        try start(source: mic, epoch: epoch)
        try start(source: system, epoch: epoch)
      } catch let fault as MeetingCaptureFault {
        fail(fault)
      } catch {
        fail(
          MeetingCaptureFault(
            code: "source_resume_failed", detail: String(describing: error)))
      }
    }
  }

  /// Stops and promotes a healthy pair. This method waits for both source stop
  /// paths and both bounded writer queues to drain.
  @discardableResult
  public func stop() -> MeetingCaptureReceipt? {
    controlQueue.sync { finish(promote: true, interrupted: false) }
  }

  /// Parent death or a broken control channel closes readable partial WAVs but
  /// never promotes them to product capture artifacts.
  public func interrupt() {
    controlQueue.sync {
      _ = finish(promote: false, interrupted: true)
    }
  }

  /// Fails a malformed application-side control without promoting partial
  /// artifacts or emitting a second, contradictory interrupted result.
  public func abort(_ fault: MeetingCaptureFault) {
    controlQueue.sync {
      _ = finish(promote: false, interrupted: false, failure: fault)
    }
  }

  private func start(source: MeetingAudioSource, epoch: UInt64) throws {
    try source.start(
      onPCM: { [weak self] data in self?.receive(data, from: source.leg, epoch: epoch) },
      onFailure: { [weak self] fault in self?.recordSource(fault, from: source.leg, epoch: epoch) })
  }

  private func receive(_ data: Data, from leg: MeetingCaptureLeg, epoch: UInt64) {
    guard !data.isEmpty else { return }

    let snapshot = lock.withLock {
      (_state, pair, stopDrainingLegs.contains(leg), acquisitionEpoch == epoch)
    }
    guard snapshot.3 else { return }
    switch snapshot.0 {
    case .arming:
      // Readiness work, including file creation, stays off the real-time audio
      // callback. Blocks arriving before the gate opens are intentionally lost.
      controlQueue.async { [weak self] in self?.markReady(leg, epoch: epoch) }
    case .resuming:
      // Same readiness rule as arming, without a second file-creation step:
      // the first block from each leg proves the source is producing and is
      // intentionally lost.
      controlQueue.async { [weak self] in self?.markResumed(leg, epoch: epoch) }
    case .recording:
      controlQueue.async { [weak self] in self?.recordBlockDelivery(from: leg, epoch: epoch) }
      append(data, to: leg, pair: snapshot.1)
    case .stopping where snapshot.2:
      // A source may synchronously emit its already-buffered remainder from
      // stop(). Accept only while that exact leg's stop path is active; once
      // stop returns, callbacks from that leg are post-stop and are rejected.
      append(data, to: leg, pair: snapshot.1)
    case .paused, .suspending, .suspended, .stopping, .terminal:
      break
    }
  }

  private func append(_ data: Data, to leg: MeetingCaptureLeg, pair: PrivateWAVPair?) {
    guard data.count.isMultiple(of: 2), let pair else {
      record(
        MeetingCaptureFault(
          code: "invalid_pcm", leg: leg,
          detail: "normalized PCM must contain whole signed 16-bit samples"))
      return
    }
    if !pair.append(data, to: leg) {
      record(
        MeetingCaptureFault(
          code: "writer_queue_overflow", leg: leg,
          detail: "bounded WAV writer queue refused audio"))
    }
  }

  private func record(_ fault: MeetingCaptureFault) {
    let shouldSchedule = lock.withLock { () -> Bool in
      guard _state != .terminal else { return false }
      if storedFault == nil { storedFault = fault }
      return _state != .stopping
    }
    if shouldSchedule {
      controlQueue.async { [weak self] in self?.fail(fault) }
    }
  }

  private func recordSource(_ fault: MeetingCaptureFault, from leg: MeetingCaptureLeg, epoch: UInt64) {
    let shouldSchedule = lock.withLock { () -> Bool in
      guard
        acquisitionEpoch == epoch,
        _state != .terminal,
        _state != .stopping || stopDrainingLegs.contains(leg)
      else { return false }
      if storedFault == nil { storedFault = fault }
      return _state != .stopping
    }
    if shouldSchedule {
      controlQueue.async { [weak self] in self?.fail(fault) }
    }
  }

  private func markReady(_ leg: MeetingCaptureLeg, epoch: UInt64) {
    guard isActiveAcquisition(epoch, state: .arming) else { return }
    readyLegs.insert(leg)
    guard readyLegs == Set(MeetingCaptureLeg.allCases) else { return }

    do {
      let newPair = try PrivateWAVPair(
        directoryFD: directoryFD,
        maxPendingBytes: maxPendingBytes,
        onFailure: { [weak self] fault in self?.record(fault) })
      lock.withLock {
        pair = newPair
        _state = .recording
      }
      armStallWatchdog(epoch: epoch)
      onUpdate(.recording)
    } catch let fault as MeetingCaptureFault {
      fail(fault)
    } catch {
      fail(
        MeetingCaptureFault(code: "wav_open_failed", detail: String(describing: error)))
    }
  }

  private func markResumed(_ leg: MeetingCaptureLeg, epoch: UInt64) {
    guard isActiveAcquisition(epoch, state: .resuming) else { return }
    readyLegs.insert(leg)
    guard readyLegs == Set(MeetingCaptureLeg.allCases) else { return }
    lock.withLock { _state = .recording }
    armStallWatchdog(epoch: epoch)
    onUpdate(.resumed)
  }

  /// All freshness bookkeeping runs on the control queue. It measures block
  /// delivery, not sample magnitude, so a source producing silent zero PCM is
  /// healthy just like one producing speech or system sound.
  private func recordBlockDelivery(from leg: MeetingCaptureLeg, epoch: UInt64) {
    guard isActiveAcquisition(epoch, state: .recording) else { return }
    lastBlockDelivery[leg] = DispatchTime.now().uptimeNanoseconds
  }

  private func armStallWatchdog(epoch: UInt64) {
    cancelStallWatchdog()
    let now = DispatchTime.now().uptimeNanoseconds
    lastBlockDelivery = Dictionary(uniqueKeysWithValues: monitoredStallLegs.map { ($0, now) })
    let interval = max(UInt64(10_000_000), min(stallGraceNanoseconds / 2, UInt64(1_000_000_000)))
    let timer = DispatchSource.makeTimerSource(queue: controlQueue)
    timer.setEventHandler { [weak self] in self?.checkForStalledAudio(epoch: epoch) }
    timer.schedule(
      deadline: .now() + .nanoseconds(Int(stallGraceNanoseconds)),
      repeating: .nanoseconds(Int(interval)))
    stallWatchdog = timer
    watchdogEpoch = epoch
    timer.resume()
  }

  private func cancelStallWatchdog() {
    stallWatchdog?.setEventHandler {}
    stallWatchdog?.cancel()
    stallWatchdog = nil
    watchdogEpoch = nil
    lastBlockDelivery = [:]
  }

  private func checkForStalledAudio(epoch: UInt64) {
    guard watchdogEpoch == epoch, isActiveAcquisition(epoch, state: .recording) else { return }
    guard lock.withLock({ storedFault == nil }) else { return }
    let now = DispatchTime.now().uptimeNanoseconds
    guard let stalledLeg = monitoredStallLegs.first(where: {
      guard let delivered = lastBlockDelivery[$0] else { return true }
      return now &- delivered >= stallGraceNanoseconds
    }) else { return }
    let code = stalledLeg == .mic ? "microphone_audio_stalled" : "system_audio_stalled"
    fail(
      MeetingCaptureFault(
        code: code, leg: stalledLeg,
        detail: "\(stalledLeg.rawValue) source delivered no PCM before its stall deadline"))
  }

  private func fail(_ fault: MeetingCaptureFault) {
    guard state != .terminal else { return }
    _ = finish(promote: false, interrupted: false, failure: fault)
  }

  private var monitoredStallLegs: [MeetingCaptureLeg] {
    monitorsSystemAudioStall ? [.mic, .system] : [.mic]
  }

  private func beginAcquisition() -> UInt64 {
    lock.withLock {
      acquisitionEpoch &+= 1
      return acquisitionEpoch
    }
  }

  private func invalidateAcquisition() {
    lock.withLock { acquisitionEpoch &+= 1 }
  }

  private func isActiveAcquisition(_ epoch: UInt64, state requiredState: MeetingCaptureState? = nil) -> Bool {
    lock.withLock {
      acquisitionEpoch == epoch && (requiredState == nil || _state == requiredState)
    }
  }

  private func finish(
    promote: Bool,
    interrupted: Bool,
    failure: MeetingCaptureFault? = nil
  ) -> MeetingCaptureReceipt? {
    let prior = state
    guard prior != .terminal else { return nil }
    cancelStallWatchdog()
    lock.withLock {
      _state = .stopping
      if pair != nil { stopDrainingLegs = Set(MeetingCaptureLeg.allCases) }
    }

    mic.stop()
    _ = lock.withLock { stopDrainingLegs.remove(.mic) }
    system.stop()
    _ = lock.withLock { stopDrainingLegs.remove(.system) }

    let activePair = lock.withLock { pair }
    var receipt: MeetingCaptureReceipt?
    var terminalFault = failure ?? lock.withLock { storedFault }
    if let activePair {
      do {
        receipt = try activePair.finish(promote: promote && terminalFault == nil && !interrupted)
      } catch let fault as MeetingCaptureFault {
        terminalFault = terminalFault ?? fault
      } catch {
        terminalFault =
          terminalFault
          ?? MeetingCaptureFault(code: "wav_finalize_failed", detail: String(describing: error))
      }
    }

    lock.withLock {
      pair = nil
      _state = .terminal
      acquisitionEpoch &+= 1
    }

    if interrupted {
      onUpdate(.interrupted)
    } else if let terminalFault {
      onUpdate(.failed(terminalFault))
    } else if let receipt, promote {
      onUpdate(.finalized(receipt))
    } else {
      onUpdate(
        .failed(
          MeetingCaptureFault(
            code: "capture_not_ready", detail: "capture stopped before both legs were ready")))
    }
    return receipt
  }

  private func transition(from: MeetingCaptureState, to: MeetingCaptureState) -> Bool {
    lock.withLock {
      guard _state == from else { return false }
      _state = to
      return true
    }
  }
}

private final class PrivateWAVPair: @unchecked Sendable {
  private let directoryFD: Int32
  private let mic: BoundedWAVWriter
  private let system: BoundedWAVWriter
  private let lock = NSLock()
  private var finished = false

  init(
    directoryFD: Int32,
    maxPendingBytes: Int,
    onFailure: @escaping @Sendable (MeetingCaptureFault) -> Void
  ) throws {
    let retainedFD = dup(directoryFD)
    guard retainedFD >= 0 else {
      throw MeetingCaptureFault(
        code: "capture_directory_unavailable", detail: "cannot retain capture directory")
    }
    self.directoryFD = retainedFD
    do {
      try Self.requireAbsent(directoryFD: retainedFD, name: "mic.wav")
      try Self.requireAbsent(directoryFD: retainedFD, name: "system.wav")
      try Self.requireAbsent(directoryFD: retainedFD, name: ".mic.wav.partial")
      try Self.requireAbsent(directoryFD: retainedFD, name: ".system.wav.partial")
      mic = try BoundedWAVWriter(
        directoryFD: retainedFD, partialName: ".mic.wav.partial", leg: .mic,
        maxPendingBytes: maxPendingBytes, onFailure: onFailure)
      do {
        system = try BoundedWAVWriter(
          directoryFD: retainedFD, partialName: ".system.wav.partial", leg: .system,
          maxPendingBytes: maxPendingBytes, onFailure: onFailure)
      } catch {
        mic.closeReadable()
        throw error
      }
    } catch {
      close(retainedFD)
      throw error
    }
  }

  deinit {
    close(directoryFD)
  }

  func append(_ data: Data, to leg: MeetingCaptureLeg) -> Bool {
    switch leg {
    case .mic: return mic.append(data)
    case .system: return system.append(data)
    }
  }

  func finish(promote: Bool) throws -> MeetingCaptureReceipt {
    let shouldFinish = lock.withLock { () -> Bool in
      guard !finished else { return false }
      finished = true
      return true
    }
    guard shouldFinish else {
      throw MeetingCaptureFault(
        code: "wav_already_finalized", detail: "capture WAV pair was finalized twice")
    }

    var firstError: Error?
    do { try mic.finish() } catch { firstError = error }
    do { try system.finish() } catch { firstError = firstError ?? error }
    if let firstError { throw firstError }

    let receipt = MeetingCaptureReceipt(
      micSamples: mic.sampleCount, systemSamples: system.sampleCount)
    if promote {
      var promotionError: Error?
      var micPromoted = false
      do {
        try Self.promote(
          directoryFD: directoryFD, partial: ".mic.wav.partial", final: "mic.wav", leg: .mic)
        micPromoted = true
        try Self.promote(
          directoryFD: directoryFD, partial: ".system.wav.partial", final: "system.wav",
          leg: .system)
      } catch {
        promotionError = error
        if micPromoted {
          do {
            try Self.rollbackPromotion(
              directoryFD: directoryFD, final: "mic.wav", partial: ".mic.wav.partial",
              leg: .mic)
          } catch {
            promotionError = error
          }
        }
      }
      guard fsync(directoryFD) == 0 else {
        throw MeetingCaptureFault(
          code: "capture_directory_sync_failed",
          detail: "capture directory could not be synchronized")
      }
      if let promotionError { throw promotionError }
    }
    return receipt
  }

  private static func requireAbsent(directoryFD: Int32, name: String) throws {
    var metadata = stat()
    if fstatat(directoryFD, name, &metadata, AT_SYMLINK_NOFOLLOW) == 0 {
      throw MeetingCaptureFault(
        code: "capture_no_overwrite", detail: "refusing to replace existing \(name)")
    }
    guard errno == ENOENT else {
      throw MeetingCaptureFault(
        code: "capture_path_check_failed", detail: "cannot inspect \(name)")
    }
  }

  private static func promote(
    directoryFD: Int32, partial: String, final: String, leg: MeetingCaptureLeg
  ) throws {
    guard renameatx_np(directoryFD, partial, directoryFD, final, UInt32(RENAME_EXCL)) == 0 else {
      throw MeetingCaptureFault(
        code: errno == EEXIST ? "capture_no_overwrite" : "wav_promote_failed", leg: leg,
        detail: "cannot promote \(partial) to \(final)")
    }
  }

  private static func rollbackPromotion(
    directoryFD: Int32, final: String, partial: String, leg: MeetingCaptureLeg
  ) throws {
    guard renameatx_np(directoryFD, final, directoryFD, partial, UInt32(RENAME_EXCL)) == 0 else {
      throw MeetingCaptureFault(
        code: "wav_promotion_rollback_failed", leg: leg,
        detail: "cannot restore the first leg after pair promotion failed")
    }
  }
}

private final class BoundedWAVWriter: @unchecked Sendable {
  private let directoryFD: Int32
  private let partialName: String
  private let leg: MeetingCaptureLeg
  private let maxPendingBytes: Int
  private let onFailure: @Sendable (MeetingCaptureFault) -> Void
  private let queue: DispatchQueue
  private let lock = NSLock()

  private var descriptor: Int32
  private var pendingBytes = 0
  private var frames = 0
  private var storedFault: MeetingCaptureFault?
  private var closing = false

  init(
    directoryFD: Int32,
    partialName: String,
    leg: MeetingCaptureLeg,
    maxPendingBytes: Int,
    onFailure: @escaping @Sendable (MeetingCaptureFault) -> Void
  ) throws {
    self.directoryFD = directoryFD
    self.partialName = partialName
    self.leg = leg
    self.maxPendingBytes = maxPendingBytes
    self.onFailure = onFailure
    queue = DispatchQueue(label: "local-meeting-notes.wav.\(leg.rawValue)")

    descriptor = openat(
      directoryFD, partialName,
      O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC | O_NOFOLLOW,
      mode_t(0o600))
    guard descriptor >= 0 else {
      throw MeetingCaptureFault(
        code: errno == EEXIST ? "capture_no_overwrite" : "wav_open_failed", leg: leg,
        detail: "cannot create private partial WAV")
    }
    guard fchmod(descriptor, mode_t(0o600)) == 0 else {
      close(descriptor)
      descriptor = -1
      throw MeetingCaptureFault(
        code: "wav_mode_failed", leg: leg, detail: "cannot enforce WAV mode 0600")
    }
    do {
      try Self.writeAll(Self.header(dataBytes: 0), to: descriptor)
    } catch {
      close(descriptor)
      descriptor = -1
      throw error
    }
  }

  var sampleCount: Int { lock.withLock { frames } }

  func append(_ data: Data) -> Bool {
    guard !data.isEmpty, data.count.isMultiple(of: 2) else { return false }
    let accepted = lock.withLock { () -> Bool in
      guard !closing, storedFault == nil else { return false }
      guard pendingBytes + data.count <= maxPendingBytes else {
        storedFault = MeetingCaptureFault(
          code: "writer_queue_overflow", leg: leg,
          detail: "bounded WAV queue exceeded \(maxPendingBytes) bytes")
        return false
      }
      pendingBytes += data.count
      return true
    }
    guard accepted else { return false }

    queue.async { [self] in
      do {
        try Self.writeAll(data, to: descriptor)
        let newFrames = lock.withLock { () -> Int in
          frames += data.count / 2
          pendingBytes -= data.count
          return frames
        }
        try patchHeader(frames: newFrames)
      } catch let fault as MeetingCaptureFault {
        record(fault)
      } catch {
        record(
          MeetingCaptureFault(
            code: "wav_write_failed", leg: leg, detail: String(describing: error)))
      }
    }
    return true
  }

  func finish() throws {
    lock.withLock { closing = true }
    queue.sync {}

    let fault = lock.withLock { storedFault }
    if descriptor >= 0 {
      if fsync(descriptor) != 0, fault == nil {
        record(
          MeetingCaptureFault(
            code: "wav_sync_failed", leg: leg, detail: "WAV could not be synchronized"))
      }
      close(descriptor)
      descriptor = -1
    }
    if let fault = lock.withLock({ storedFault }) { throw fault }
    try verifyReadable()
  }

  func closeReadable() {
    lock.withLock { closing = true }
    queue.sync {}
    if descriptor >= 0 {
      _ = fsync(descriptor)
      close(descriptor)
      descriptor = -1
    }
  }

  private func patchHeader(frames: Int) throws {
    let byteCount = frames * 2
    guard byteCount <= Int(UInt32.max) - 36 else {
      throw MeetingCaptureFault(
        code: "wav_too_large", leg: leg, detail: "WAV exceeded the RIFF size limit")
    }
    var riffSize = UInt32(36 + byteCount).littleEndian
    var dataSize = UInt32(byteCount).littleEndian
    guard
      withUnsafeBytes(of: &riffSize, { pwrite(descriptor, $0.baseAddress, 4, 4) }) == 4,
      withUnsafeBytes(of: &dataSize, { pwrite(descriptor, $0.baseAddress, 4, 40) }) == 4
    else {
      throw MeetingCaptureFault(
        code: "wav_header_failed", leg: leg, detail: "cannot update WAV frame counts")
    }
  }

  private func verifyReadable() throws {
    let fd = openat(directoryFD, partialName, O_RDONLY | O_CLOEXEC | O_NOFOLLOW)
    guard fd >= 0 else {
      throw MeetingCaptureFault(
        code: "wav_readback_failed", leg: leg, detail: "final WAV cannot be reopened")
    }
    defer { close(fd) }

    var metadata = stat()
    let expectedBytes = 44 + sampleCount * 2
    guard fstat(fd, &metadata) == 0, metadata.st_size == expectedBytes else {
      throw MeetingCaptureFault(
        code: "wav_readback_failed", leg: leg,
        detail: "final WAV byte count disagrees with written samples")
    }
    var header = [UInt8](repeating: 0, count: 44)
    guard pread(fd, &header, header.count, 0) == header.count else {
      throw MeetingCaptureFault(
        code: "wav_readback_failed", leg: leg, detail: "final WAV header is unreadable")
    }
    guard
      String(bytes: header[0..<4], encoding: .ascii) == "RIFF",
      String(bytes: header[8..<12], encoding: .ascii) == "WAVE",
      header[20] == 1, header[21] == 0,
      header[22] == 1, header[23] == 0,
      Self.readUInt32(header, at: 24) == 16_000,
      header[34] == 16, header[35] == 0,
      Self.readUInt32(header, at: 40) == UInt32(sampleCount * 2)
    else {
      throw MeetingCaptureFault(
        code: "wav_readback_failed", leg: leg,
        detail: "final WAV is not mono 16 kHz signed 16-bit PCM")
    }
  }

  private func record(_ fault: MeetingCaptureFault) {
    let first = lock.withLock { () -> Bool in
      if storedFault != nil { return false }
      storedFault = fault
      return true
    }
    if first { onFailure(fault) }
  }

  private static func writeAll(_ data: Data, to descriptor: Int32) throws {
    try data.withUnsafeBytes { bytes in
      guard let base = bytes.baseAddress else { return }
      var written = 0
      while written < bytes.count {
        let result = Darwin.write(descriptor, base.advanced(by: written), bytes.count - written)
        if result > 0 {
          written += result
        } else if result < 0, errno == EINTR {
          continue
        } else {
          throw MeetingCaptureFault(code: "wav_write_failed", detail: "WAV write failed")
        }
      }
    }
  }

  private static func header(dataBytes: UInt32) -> Data {
    var result = Data()
    result.append(contentsOf: "RIFF".utf8)
    append(UInt32(36) + dataBytes, to: &result)
    result.append(contentsOf: "WAVEfmt ".utf8)
    append(UInt32(16), to: &result)
    append(UInt16(1), to: &result)
    append(UInt16(1), to: &result)
    append(UInt32(16_000), to: &result)
    append(UInt32(32_000), to: &result)
    append(UInt16(2), to: &result)
    append(UInt16(16), to: &result)
    result.append(contentsOf: "data".utf8)
    append(dataBytes, to: &result)
    return result
  }

  private static func append<T: FixedWidthInteger>(_ value: T, to data: inout Data) {
    var little = value.littleEndian
    withUnsafeBytes(of: &little) { data.append(contentsOf: $0) }
  }

  private static func readUInt32(_ bytes: [UInt8], at offset: Int) -> UInt32 {
    UInt32(bytes[offset])
      | UInt32(bytes[offset + 1]) << 8
      | UInt32(bytes[offset + 2]) << 16
      | UInt32(bytes[offset + 3]) << 24
  }
}

extension NSLock {
  fileprivate func withLock<T>(_ body: () throws -> T) rethrows -> T {
    lock()
    defer { unlock() }
    return try body()
  }
}
