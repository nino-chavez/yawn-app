import Accelerate
import Foundation

/// Makes replay loud enough to hear. The iPad records a room quietly
/// (measurement mode put speech near -58 dBFS in the microphone test), and
/// AVAudioPlayer cannot play above the file's own level. The saved audio is
/// never changed: replay plays a boosted copy held in memory.
public enum ReplayBoost {
    /// Where speech (the 90th percentile of 100 ms window levels) lands.
    public static let targetSpeechDBFS = -20.0
    public static let maxGainDB = 40.0
    /// The soft limiter's ceiling, just under full scale.
    static let ceiling: Float = 0.99

    /// Gain that brings speech to the target, never a cut, and at most maxGainDB.
    public static func gainDB(speechDBFS: Double) -> Double {
        min(maxGainDB, max(0, targetSpeechDBFS - speechDBFS))
    }

    /// A chunk that is a whole number of windows, so levels measured chunk by
    /// chunk equal levels measured over the whole file.
    public static let chunkFrames = LevelAccumulator.windowSamples * 100

    /// RMS in dBFS of each complete 100 ms window; a trailing partial window
    /// is skipped, as LevelAccumulator does.
    public static func windowLevels(_ samples: UnsafeBufferPointer<Float>) -> [Double] {
        let window = LevelAccumulator.windowSamples
        guard let base = samples.baseAddress, samples.count >= window else { return [] }
        return stride(from: 0, through: samples.count - window, by: window).map { start in
            var rms: Float = 0
            vDSP_rmsqv(base + start, 1, &rms, vDSP_Length(window))
            return LevelAccumulator.dbfs(Double(rms))
        }
    }

    /// The 90th percentile of 100 ms window RMS, the same measure
    /// recording-check.json reports as window_rms_p90_dbfs.
    public static func speechLevel(windowLevels levels: [Double]) -> Double {
        LevelAccumulator.percentile(levels.sorted(), 0.90)
    }

    public static func speechLevel(_ samples: [Float]) -> Double {
        speechLevel(windowLevels: samples.withUnsafeBufferPointer { windowLevels($0) })
    }

    /// Applies the gain, then a tanh soft limiter that keeps every sample under
    /// the ceiling, and returns 16-bit samples.
    public static func boosted(_ samples: [Float], gainDB: Double) -> [Int16] {
        samples.withUnsafeBufferPointer { boosted($0, gainDB: gainDB) }
    }

    public static func boosted(_ samples: UnsafeBufferPointer<Float>, gainDB: Double) -> [Int16] {
        guard let input = samples.baseAddress, !samples.isEmpty else { return [] }
        let count = samples.count
        var scaled = [Float](repeating: 0, count: count)
        // ceiling * tanh(x * gain / ceiling): near-linear for quiet speech,
        // smoothly bounded for loud peaks.
        var factor = Float(pow(10, gainDB / 20)) / ceiling
        vDSP_vsmul(input, 1, &factor, &scaled, 1, vDSP_Length(count))
        var limited = [Float](repeating: 0, count: count)
        var length = Int32(count)
        vvtanhf(&limited, scaled, &length)
        var toInt = ceiling * Float(Int16.max)
        vDSP_vsmul(limited, 1, &toInt, &scaled, 1, vDSP_Length(count))
        var output = [Int16](repeating: 0, count: count)
        vDSP_vfixr16(scaled, 1, &output, 1, vDSP_Length(count))
        return output
    }

    /// A 16-bit mono PCM WAV file in memory, for AVAudioPlayer(data:).
    public static func wavData(_ samples: [Int16], sampleRate: Int) -> Data {
        var data = wavHeader(sampleCount: samples.count, sampleRate: sampleRate)
        appendSamples(samples, to: &data)
        return data
    }

    /// Appends 16-bit samples after a header. WAV is little-endian, as every
    /// Apple platform is; one bulk copy per chunk keeps this fast.
    public static func appendSamples(_ samples: [Int16], to data: inout Data) {
        #if _endian(little)
        samples.withUnsafeBytes { data.append(contentsOf: $0) }
        #else
        for sample in samples { withUnsafeBytes(of: sample.littleEndian) { data.append(contentsOf: $0) } }
        #endif
    }

    /// The 44-byte header of a 16-bit mono PCM WAV with sampleCount samples.
    public static func wavHeader(sampleCount: Int, sampleRate: Int) -> Data {
        let dataBytes = sampleCount * 2
        var data = Data(capacity: 44 + dataBytes)
        func append<T: FixedWidthInteger>(_ value: T) {
            withUnsafeBytes(of: value.littleEndian) { data.append(contentsOf: $0) }
        }
        data.append(contentsOf: Array("RIFF".utf8))
        append(UInt32(36 + dataBytes))
        data.append(contentsOf: Array("WAVE".utf8))
        data.append(contentsOf: Array("fmt ".utf8))
        append(UInt32(16))
        append(UInt16(1))
        append(UInt16(1))
        append(UInt32(sampleRate))
        append(UInt32(sampleRate * 2))
        append(UInt16(2))
        append(UInt16(16))
        data.append(contentsOf: Array("data".utf8))
        append(UInt32(dataBytes))
        return data
    }
}
