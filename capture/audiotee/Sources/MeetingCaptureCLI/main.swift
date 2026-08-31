import AVFoundation
import AudioTeeCore
import CoreAudio
import Darwin
import Foundation

private let captureRate = 16_000.0

private enum CaptureMode {
  /// Two-leg meeting capture writing the private WAV pair into the capture dir.
  case meeting(captureDirectoryFD: Int32)
  /// One-leg voice-profile sitting streaming microphone PCM over a pipe; the
  /// application's evidence store owns every byte that becomes durable.
  case sitting(audioFD: Int32)
}

/// The only permission modes the real capture executable supports. They are
/// deliberately separate from the four-descriptor capture contract: preflight
/// never receives a capture directory, never starts an audio engine, and never
/// writes a product record.
private enum PermissionPreflightMode: String {
  case status
  case requestMicrophone = "request-microphone"
  case requestSystemAudio = "request-system-audio"
}

private enum Invocation {
  case capture(Options)
  case permissionPreflight(PermissionPreflightMode)

  static func parse(_ arguments: [String]) throws -> Self {
    if arguments.first == "--permission-preflight" {
      guard arguments.count == 2,
        let mode = PermissionPreflightMode(rawValue: arguments[1])
      else {
        throw MeetingCaptureFault(
          code: "invalid_arguments",
          detail: "meeting-capture permission preflight expects status, request-microphone, or request-system-audio")
      }
      return .permissionPreflight(mode)
    }
    return .capture(try Options.parse(arguments))
  }
}

private struct Options {
  let mode: CaptureMode
  let controlFD: Int32
  let eventFD: Int32
  let parentLivenessFD: Int32

  static func parse(_ arguments: [String]) throws -> Options {
    let allowed = Set([
      "--capture-dir-fd", "--sitting-audio-fd",
      "--control-fd", "--event-fd", "--parent-liveness-fd",
    ])
    var values: [String: Int32] = [:]
    var index = 0
    while index < arguments.count {
      let name = arguments[index]
      guard allowed.contains(name), values[name] == nil, index + 1 < arguments.count,
        let value = Int32(arguments[index + 1]), value >= 0
      else {
        throw MeetingCaptureFault(
          code: "invalid_arguments",
          detail: "meeting-capture accepts exactly four inherited file descriptors")
      }
      values[name] = value
      index += 2
    }
    let mode: CaptureMode
    switch (values["--capture-dir-fd"], values["--sitting-audio-fd"]) {
    case (let directory?, nil):
      mode = .meeting(captureDirectoryFD: directory)
    case (nil, let audio?):
      mode = .sitting(audioFD: audio)
    default:
      throw MeetingCaptureFault(
        code: "invalid_arguments",
        detail: "meeting-capture requires exactly one of the capture-dir and "
          + "sitting-audio descriptors")
    }
    guard values.count == 4 else {
      throw MeetingCaptureFault(
        code: "invalid_arguments",
        detail: "meeting-capture requires capture, control, event, and liveness descriptors")
    }
    let descriptors = Set(values.values)
    guard descriptors.count == values.count else {
      throw MeetingCaptureFault(
        code: "invalid_arguments", detail: "meeting-capture descriptors must be distinct")
    }
    return Options(
      mode: mode,
      controlFD: values["--control-fd"]!,
      eventFD: values["--event-fd"]!,
      parentLivenessFD: values["--parent-liveness-fd"]!)
  }
}

private let permissionSchema = "permission-probe/1"

private func microphonePermissionStatus() -> String {
  switch AVCaptureDevice.authorizationStatus(for: .audio) {
  case .authorized: return "authorized"
  case .denied: return "denied"
  case .restricted: return "restricted"
  case .notDetermined: return "not-determined"
  @unknown default: return "unknown"
  }
}

private func emitPermissionResult(_ payload: [String: Any]) throws -> Int32 {
  var complete = payload
  complete["schema"] = permissionSchema
  guard
    let data = try? JSONSerialization.data(
      withJSONObject: complete, options: [.sortedKeys]),
    let line = String(data: data, encoding: .utf8)
  else {
    throw MeetingCaptureFault(
      code: "permission_preflight_encode_failed",
      detail: "could not encode the capture permission result")
  }
  print(line)
  return 0
}

private func requestMicrophonePermission() throws -> (status: String, prompted: Bool) {
  let before = microphonePermissionStatus()
  guard before == "not-determined" else { return (before, false) }
  let settled = DispatchSemaphore(value: 0)
  AVCaptureDevice.requestAccess(for: .audio) { _ in settled.signal() }
  guard settled.wait(timeout: .now() + 120) != .timedOut else {
    throw MeetingCaptureFault(
      code: "microphone_permission_timeout",
      detail: "the microphone prompt was not answered within 120 seconds")
  }
  return (microphonePermissionStatus(), true)
}

/// The single configuration the real meeting recorder uses for system audio.
/// Preflight calls the same `AudioTapManager` setup with this exact value so a
/// green setup state means the helper that will record was the thing measured.
private func meetingSystemTapConfiguration() -> TapConfiguration {
  TapConfiguration(
    processes: [], muteBehavior: .unmuted, isExclusive: true, isMono: true)
}

private func requestSystemAudioPermission() -> String {
  guard #available(macOS 14.4, *) else { return "unsupported" }
  let manager = AudioTapManager()
  do {
    try manager.setupAudioTap(with: meetingSystemTapConfiguration())
    manager.teardown()
    return "authorized"
  } catch {
    manager.teardown()
    return "unavailable"
  }
}

/// Checks permission from the same signed helper and through the same system-audio
/// setup path that a meeting will use. It never constructs an `AudioRecorder`,
/// starts an audio device, reads a buffer, opens a capture directory, or writes a
/// file.
private func runPermissionPreflight(_ mode: PermissionPreflightMode) throws -> Int32 {
  switch mode {
  case .status:
    return try emitPermissionResult([
      "action": "status",
      "microphone": microphonePermissionStatus(),
      "system_audio": "unmeasured",
      "system_audio_detail": "run request-system-audio; setup uses the real capture helper",
    ])
  case .requestMicrophone:
    let result = try requestMicrophonePermission()
    return try emitPermissionResult([
      "action": "request-microphone",
      "microphone": result.status,
      "prompted": result.prompted,
    ])
  case .requestSystemAudio:
    return try emitPermissionResult([
      "action": "request-system-audio",
      "system_audio": requestSystemAudioPermission(),
    ])
  }
}

private final class EventWriter: @unchecked Sendable {
  private let descriptor: Int32
  private let lock = NSLock()

  init(descriptor: Int32) throws {
    guard fcntl(descriptor, F_GETFD) != -1 else {
      throw MeetingCaptureFault(
        code: "event_channel_unavailable", detail: "event descriptor is invalid")
    }
    self.descriptor = descriptor
  }

  @discardableResult
  func emit(event: String, extra: [String: Any] = [:]) -> Bool {
    var payload: [String: Any] = ["schema": "capture-event/1", "event": event]
    for (key, value) in extra { payload[key] = value }
    guard JSONSerialization.isValidJSONObject(payload),
      var encoded = try? JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
    else { return false }
    encoded.append(0x0A)

    return lock.withLock {
      encoded.withUnsafeBytes { bytes in
        guard let base = bytes.baseAddress else { return true }
        var written = 0
        while written < bytes.count {
          let result = Darwin.write(
            descriptor, base.advanced(by: written), bytes.count - written)
          if result > 0 {
            written += result
          } else if result < 0, errno == EINTR {
            continue
          } else {
            return false
          }
        }
        return true
      }
    }
  }
}

private final class TerminalState: @unchecked Sendable {
  private let lock = NSLock()
  private var value: Int32?

  func set(_ code: Int32) { lock.withLock { value = value ?? code } }
  func get() -> Int32? { lock.withLock { value } }
}

private final class SystemMeetingAudioSource: MeetingAudioSource, @unchecked Sendable {
  let leg = MeetingCaptureLeg.system
  private let lock = NSLock()
  private var manager: AudioTapManager?
  private var recorder: AudioRecorder?

  func start(
    onPCM: @escaping @Sendable (Data) -> Void,
    onFailure: @escaping @Sendable (MeetingCaptureFault) -> Void
  ) throws {
    let manager = AudioTapManager()
    do {
      try manager.setupAudioTap(with: meetingSystemTapConfiguration())
    } catch {
      throw MeetingCaptureFault(
        code: "system_tap_setup_failed", leg: .system, detail: String(describing: error))
    }
    guard let deviceID = manager.getDeviceID() else {
      throw MeetingCaptureFault(
        code: "system_tap_unavailable", leg: .system,
        detail: "system-audio tap did not create a capture device")
    }
    let handler = StrictSystemOutput(onPCM: onPCM, onFailure: onFailure)
    let recorder: AudioRecorder
    do {
      recorder = try AudioRecorder(
        deviceID: deviceID, outputHandler: handler, convertToSampleRate: captureRate,
        chunkDuration: 0.2, strictConversion: true)
    } catch {
      throw MeetingCaptureFault(
        code: "system_conversion_unavailable", leg: .system, detail: String(describing: error))
    }
    let format = recorder.outputFormat
    guard format.mSampleRate == captureRate, format.mChannelsPerFrame == 1,
      format.mBitsPerChannel == 16,
      format.mFormatFlags & kAudioFormatFlagIsSignedInteger != 0,
      format.mFormatFlags & kAudioFormatFlagIsFloat == 0
    else {
      throw MeetingCaptureFault(
        code: "system_format_invalid", leg: .system,
        detail: "system tap did not normalize to mono 16 kHz signed 16-bit PCM")
    }
    lock.withLock {
      self.manager = manager
      self.recorder = recorder
    }
    do {
      try recorder.startRecording()
    } catch {
      lock.withLock {
        self.recorder = nil
        self.manager = nil
      }
      throw MeetingCaptureFault(
        code: "system_tap_start_failed", leg: .system, detail: String(describing: error))
    }
  }

  func stop() {
    let active = lock.withLock { () -> AudioRecorder? in
      let value = recorder
      recorder = nil
      return value
    }
    active?.stopRecording()
    lock.withLock { manager = nil }
  }
}

private final class StrictSystemOutput: AudioOutputHandler, @unchecked Sendable {
  private let onPCM: @Sendable (Data) -> Void
  private let onFailure: @Sendable (MeetingCaptureFault) -> Void

  init(
    onPCM: @escaping @Sendable (Data) -> Void,
    onFailure: @escaping @Sendable (MeetingCaptureFault) -> Void
  ) {
    self.onPCM = onPCM
    self.onFailure = onFailure
  }

  func handleAudioData(_ pointer: UnsafeRawPointer, count: Int) {
    guard count > 0, count.isMultiple(of: 2) else {
      onFailure(
        MeetingCaptureFault(
          code: "system_invalid_pcm", leg: .system,
          detail: "system converter emitted an invalid PCM block"))
      return
    }
    onPCM(Data(bytes: pointer, count: count))
  }

  func handleMetadata(_ metadata: AudioStreamMetadata) {}
  func handleStreamStart() {}
  func handleStreamStop() {}

  func handleFailure(_ failure: AudioOutputFailure) {
    let code: String
    switch failure {
    case .bufferOverflow: code = "system_buffer_overflow"
    case .conversionFailed: code = "system_conversion_failed"
    case .timelineDiscontinuity: code = "system_timeline_discontinuity"
    }
    onFailure(MeetingCaptureFault(code: code, leg: .system, detail: String(describing: failure)))
  }
}

private final class MicrophoneMeetingAudioSource: MeetingAudioSource, @unchecked Sendable {
  let leg = MeetingCaptureLeg.mic
  private let lock = NSLock()
  private let setupQueue = DispatchQueue(label: "local-meeting-notes.microphone-setup")
  private var engine: AVAudioEngine?
  private var observer: NSObjectProtocol?
  private var cancelled = false
  private var failed = false
  private var expectedSampleTime: AVAudioFramePosition?
  private var failureHandler: (@Sendable (MeetingCaptureFault) -> Void)?

  func start(
    onPCM: @escaping @Sendable (Data) -> Void,
    onFailure: @escaping @Sendable (MeetingCaptureFault) -> Void
  ) throws {
    let mayStart = lock.withLock { () -> Bool in
      guard engine == nil, failureHandler == nil else { return false }
      failureHandler = onFailure
      cancelled = false
      failed = false
      return true
    }
    guard mayStart else {
      throw MeetingCaptureFault(
        code: "mic_duplicate_start", leg: .mic,
        detail: "microphone source was started more than once")
    }

    switch AVCaptureDevice.authorizationStatus(for: .audio) {
    case .authorized:
      setupQueue.async { [weak self] in self?.startEngine(onPCM: onPCM) }
    case .notDetermined:
      AVCaptureDevice.requestAccess(for: .audio) { [weak self] granted in
        guard let self else { return }
        if granted {
          setupQueue.async { [weak self] in self?.startEngine(onPCM: onPCM) }
        } else {
          report(
            MeetingCaptureFault(
              code: "microphone_permission_denied", leg: .mic,
              detail: "microphone permission was not granted"))
        }
      }
    case .denied, .restricted:
      report(
        MeetingCaptureFault(
          code: "microphone_permission_denied", leg: .mic,
          detail: "microphone permission is unavailable"))
    @unknown default:
      report(
        MeetingCaptureFault(
          code: "microphone_permission_unknown", leg: .mic,
          detail: "microphone permission state is unknown"))
    }
  }

  func stop() {
    let active = lock.withLock { () -> (AVAudioEngine?, NSObjectProtocol?) in
      cancelled = true
      let values = (engine, observer)
      engine = nil
      observer = nil
      failureHandler = nil
      // A stopped engine ends its sample timeline. Carrying the old expectation
      // into a later start would read the new engine's first buffer as a gap
      // and fail the capture, so the source forgets it here rather than at
      // start, where a fresh instance would also have to know to clear it.
      expectedSampleTime = nil
      return values
    }
    if let observer = active.1 { NotificationCenter.default.removeObserver(observer) }
    if let engine = active.0 {
      engine.inputNode.removeTap(onBus: 0)
      engine.stop()
    }
  }

  private func startEngine(onPCM: @escaping @Sendable (Data) -> Void) {
    guard !lock.withLock({ cancelled }) else { return }
    let engine = AVAudioEngine()
    let input = engine.inputNode
    let sourceFormat = input.outputFormat(forBus: 0)
    guard sourceFormat.sampleRate > 0, sourceFormat.channelCount > 0 else {
      report(
        MeetingCaptureFault(
          code: "microphone_unavailable", leg: .mic,
          detail: "microphone has no readable input format"))
      return
    }
    let settings: [String: Any] = [
      AVFormatIDKey: kAudioFormatLinearPCM,
      AVSampleRateKey: captureRate,
      AVNumberOfChannelsKey: 1,
      AVLinearPCMBitDepthKey: 16,
      AVLinearPCMIsFloatKey: false,
      AVLinearPCMIsBigEndianKey: false,
      AVLinearPCMIsNonInterleaved: false,
    ]
    guard let targetFormat = AVAudioFormat(settings: settings),
      targetFormat.sampleRate == captureRate,
      targetFormat.channelCount == 1,
      targetFormat.commonFormat == .pcmFormatInt16,
      targetFormat.isInterleaved,
      let converter = AVAudioConverter(from: sourceFormat, to: targetFormat)
    else {
      report(
        MeetingCaptureFault(
          code: "microphone_conversion_unavailable", leg: .mic,
          detail: "microphone cannot normalize to mono 16 kHz signed 16-bit PCM"))
      return
    }
    if sourceFormat.channelCount > 1 { converter.channelMap = [0] }

    input.installTap(onBus: 0, bufferSize: 4096, format: sourceFormat) {
      [weak self] buffer, time in
      guard let self, !self.lock.withLock({ self.cancelled || self.failed }) else { return }
      if time.isSampleTimeValid {
        let discontinuity = self.lock.withLock { () -> Bool in
          let mismatch = self.expectedSampleTime.map { $0 != time.sampleTime } ?? false
          self.expectedSampleTime = time.sampleTime + AVAudioFramePosition(buffer.frameLength)
          return mismatch
        }
        if discontinuity {
          report(
            MeetingCaptureFault(
              code: "microphone_timeline_discontinuity", leg: .mic,
              detail: "microphone sample timeline contains a gap"))
          return
        }
      }

      let ratio = captureRate / sourceFormat.sampleRate
      let capacity = AVAudioFrameCount(ceil(Double(buffer.frameLength) * ratio)) + 32
      guard
        let output = AVAudioPCMBuffer(
          pcmFormat: targetFormat, frameCapacity: max(capacity, 1))
      else {
        report(
          MeetingCaptureFault(
            code: "microphone_conversion_failed", leg: .mic,
            detail: "microphone conversion buffer allocation failed"))
        return
      }
      var supplied = false
      var conversionError: NSError?
      let status = converter.convert(to: output, error: &conversionError) { _, state in
        if supplied {
          state.pointee = .noDataNow
          return nil
        }
        supplied = true
        state.pointee = .haveData
        return buffer
      }
      let audio = output.audioBufferList.pointee.mBuffers
      guard status != .error, output.frameLength > 0, let bytes = audio.mData,
        audio.mDataByteSize > 0, Int(audio.mDataByteSize).isMultiple(of: 2)
      else {
        report(
          MeetingCaptureFault(
            code: "microphone_conversion_failed", leg: .mic,
            detail: conversionError?.localizedDescription ?? "microphone converter emitted no PCM"))
        return
      }
      onPCM(Data(bytes: bytes, count: Int(audio.mDataByteSize)))
    }

    let observer = NotificationCenter.default.addObserver(
      forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil
    ) { [weak self] _ in
      self?.report(
        MeetingCaptureFault(
          code: "microphone_configuration_changed", leg: .mic,
          detail: "microphone configuration changed during capture"))
    }

    do {
      try engine.start()
    } catch {
      input.removeTap(onBus: 0)
      NotificationCenter.default.removeObserver(observer)
      report(
        MeetingCaptureFault(
          code: "microphone_start_failed", leg: .mic, detail: String(describing: error)))
      return
    }
    let accepted = lock.withLock { () -> Bool in
      guard !cancelled else { return false }
      self.engine = engine
      self.observer = observer
      return true
    }
    if !accepted {
      input.removeTap(onBus: 0)
      engine.stop()
      NotificationCenter.default.removeObserver(observer)
    }
  }

  private func report(_ fault: MeetingCaptureFault) {
    let handler = lock.withLock { () -> (@Sendable (MeetingCaptureFault) -> Void)? in
      guard !failed, !cancelled else { return nil }
      failed = true
      return failureHandler
    }
    handler?(fault)
  }
}

extension NSLock {
  fileprivate func withLock<T>(_ body: () throws -> T) rethrows -> T {
    lock()
    defer { unlock() }
    return try body()
  }
}

private func run() throws -> Int32 {
  signal(SIGPIPE, SIG_IGN)
  let invocation = try Invocation.parse(Array(CommandLine.arguments.dropFirst()))
  if case let .permissionPreflight(mode) = invocation {
    return try runPermissionPreflight(mode)
  }
  guard case let .capture(options) = invocation else {
    throw MeetingCaptureFault(
      code: "invalid_arguments", detail: "meeting-capture could not determine its invocation mode")
  }
  for descriptor in [
    options.controlFD, options.eventFD, options.parentLivenessFD,
  ] where fcntl(descriptor, F_GETFD) == -1 {
    throw MeetingCaptureFault(
      code: "channel_unavailable", detail: "an inherited channel descriptor is invalid")
  }

  let events = try EventWriter(descriptor: options.eventFD)
  let terminal = TerminalState()

  @Sendable func emitRecording() {
    if !events.emit(
      event: "recording",
      extra: [
        "format": ["encoding": "pcm_s16le", "sample_rate": 16_000, "channels": 1]
      ])
    {
      terminal.set(2)
    }
  }
  // The product word for this is "paused", but the event vocabulary already
  // spends "paused" on the pre-start safe state the helper emits before it has
  // opened anything. These two name the mid-take gap instead.
  @Sendable func emitSuspended() {
    if !events.emit(event: "suspended") { terminal.set(2) }
  }
  @Sendable func emitResumed() {
    if !events.emit(event: "resumed") { terminal.set(2) }
  }
  @Sendable func emitFinalized(legs: [String: Any]) {
    let sent = events.emit(event: "finalized", extra: ["legs": legs])
    terminal.set(sent ? 0 : 2)
  }
  @Sendable func emitFailed(_ fault: MeetingCaptureFault) {
    var evidence: [String: Any] = ["code": fault.code, "detail": fault.detail]
    if let leg = fault.leg { evidence["leg"] = leg.rawValue }
    let sent = events.emit(event: "failed", extra: evidence)
    let protocolFault = fault.code == "invalid_control" || fault.code == "invalid_start"
    terminal.set(sent ? (protocolFault ? 2 : 1) : 2)
  }
  @Sendable func emitInterrupted() {
    terminal.set(events.emit(event: "interrupted") ? 1 : 2)
  }

  let activate: () -> Void
  let suspendCapture: () -> Void
  let resumeCapture: () -> Void
  let stopCapture: () -> Void
  let interrupt: () -> Void
  let abort: (MeetingCaptureFault) -> Void
  let isTerminal: () -> Bool

  switch options.mode {
  case .meeting(let captureDirectoryFD):
    let coordinator = try MeetingCaptureCoordinator(
      directoryFD: captureDirectoryFD,
      mic: MicrophoneMeetingAudioSource(),
      system: SystemMeetingAudioSource()
    ) { update in
      switch update {
      case .recording:
        emitRecording()
      case .suspended:
        emitSuspended()
      case .resumed:
        emitResumed()
      case .finalized(let receipt):
        emitFinalized(legs: [
          "mic": ["samples": receipt.micSamples],
          "system": ["samples": receipt.systemSamples],
        ])
      case .failed(let fault):
        emitFailed(fault)
      case .interrupted:
        emitInterrupted()
      }
    }
    activate = { coordinator.activate() }
    suspendCapture = {
      guard coordinator.suspend() else {
        coordinator.abort(
          MeetingCaptureFault(
            code: "invalid_control", detail: "pause is valid only while recording"))
        return
      }
    }
    resumeCapture = { coordinator.resume() }
    stopCapture = { _ = coordinator.stop() }
    interrupt = { coordinator.interrupt() }
    abort = { coordinator.abort($0) }
    isTerminal = { coordinator.state == .terminal }
  case .sitting(let audioFD):
    // Same event vocabulary with a one-leg receipt: the parent's event parser
    // stays uniform, and the absent system entry states the mode honestly.
    let coordinator = try SittingCaptureCoordinator(
      audioFD: audioFD,
      mic: MicrophoneMeetingAudioSource()
    ) { update in
      switch update {
      case .recording:
        emitRecording()
      case .finalized(let receipt):
        emitFinalized(legs: ["mic": ["samples": receipt.micSamples]])
      case .failed(let fault):
        emitFailed(fault)
      case .interrupted:
        emitInterrupted()
      }
    }
    activate = { coordinator.activate() }
    // Pause belongs to a meeting, not to a setup sitting. Refusing here keeps
    // the control protocol single-valued rather than silently ignored.
    suspendCapture = {
      coordinator.abort(
        MeetingCaptureFault(
          code: "invalid_control", detail: "a setup recording cannot be paused"))
    }
    resumeCapture = {
      coordinator.abort(
        MeetingCaptureFault(
          code: "invalid_control", detail: "a setup recording cannot be resumed"))
    }
    stopCapture = { _ = coordinator.stop() }
    interrupt = { coordinator.interrupt() }
    abort = { coordinator.abort($0) }
    isTerminal = { coordinator.state == .terminal }
  }
  guard events.emit(event: "paused") else { return 2 }

  var descriptors = [
    pollfd(fd: options.controlFD, events: Int16(POLLIN | POLLHUP | POLLERR), revents: 0),
    pollfd(
      fd: options.parentLivenessFD, events: Int16(POLLIN | POLLHUP | POLLERR), revents: 0),
  ]
  while terminal.get() == nil {
    descriptors[0].revents = 0
    descriptors[1].revents = 0
    let result = poll(&descriptors, nfds_t(descriptors.count), 100)
    if result < 0 {
      if errno == EINTR { continue }
      interrupt()
      return 2
    }
    if result == 0 { continue }

    if descriptors[1].revents & Int16(POLLIN | POLLHUP | POLLERR) != 0 {
      var byte: UInt8 = 0
      if read(options.parentLivenessFD, &byte, 1) <= 0 {
        interrupt()
        break
      }
    }
    if descriptors[0].revents & Int16(POLLIN | POLLHUP | POLLERR) != 0 {
      var command: UInt8 = 0
      let count = read(options.controlFD, &command, 1)
      if count <= 0 {
        interrupt()
        break
      }
      switch command {
      case Character("S").asciiValue:
        activate()
      case Character("P").asciiValue:
        suspendCapture()
      case Character("R").asciiValue:
        resumeCapture()
      case Character("X").asciiValue:
        stopCapture()
      default:
        abort(
          MeetingCaptureFault(
            code: "invalid_control", detail: "unknown capture control byte"))
      }
    }
  }
  if !isTerminal() { interrupt() }
  return terminal.get() ?? 1
}

do {
  exit(try run())
} catch let fault as MeetingCaptureFault {
  FileHandle.standardError.write(Data("meeting-capture: \(fault.code)\n".utf8))
  exit(2)
} catch {
  FileHandle.standardError.write(Data("meeting-capture: startup failed\n".utf8))
  exit(2)
}
