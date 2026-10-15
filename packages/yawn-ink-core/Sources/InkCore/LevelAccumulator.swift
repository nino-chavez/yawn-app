import Foundation

/// Level statistics for the 16 kHz mono audio the app writes: the same
/// samples the transcriber and the Mac speaker labelers read.
public struct LevelAccumulator: Sendable {
    /// 100 ms at 16 kHz.
    public static let windowSamples = 1_600
    public static let windowSeconds = 0.1
    /// Silence reports this instead of -infinity, so the JSON stays finite.
    public static let floorDBFS = -120.0
    /// A sample at or above this magnitude counts as clipped.
    public static let clipThreshold: Float = 0.999

    private var windowSumSquares = 0.0
    private var windowCount = 0
    private var windowPeak: Float = 0
    private var windowLevels: [Double] = []
    private var totalSumSquares = 0.0
    private var totalCount = 0
    private var peak: Float = 0
    private var clipped = 0

    /// RMS of the last complete 100 ms window, for a live meter.
    public private(set) var latestWindowRMS = floorDBFS

    public init() {}

    public mutating func add<S: Sequence>(_ samples: S) where S.Element == Float {
        for sample in samples {
            let magnitude = abs(sample)
            let square = Double(sample) * Double(sample)
            windowSumSquares += square
            totalSumSquares += square
            windowCount += 1
            totalCount += 1
            windowPeak = max(windowPeak, magnitude)
            peak = max(peak, magnitude)
            if magnitude >= Self.clipThreshold { clipped += 1 }
            if windowCount == Self.windowSamples {
                let level = Self.dbfs(sqrt(windowSumSquares / Double(windowCount)))
                windowLevels.append(level)
                latestWindowRMS = level
                windowSumSquares = 0
                windowCount = 0
                windowPeak = 0
            }
        }
    }

    public var summary: LevelSummary {
        let sorted = windowLevels.sorted()
        let p10 = Self.percentile(sorted, 0.10)
        let p90 = Self.percentile(sorted, 0.90)
        return LevelSummary(
            samples: totalCount,
            windows: sorted.count,
            rmsDBFS: Self.rounded(Self.dbfs(totalCount > 0 ? sqrt(totalSumSquares / Double(totalCount)) : 0)),
            peakDBFS: Self.rounded(Self.dbfs(Double(peak))),
            windowRMSP10DBFS: Self.rounded(p10),
            windowRMSP50DBFS: Self.rounded(Self.percentile(sorted, 0.50)),
            windowRMSP90DBFS: Self.rounded(p90),
            speechMinusNoiseDB: Self.rounded(p90 - p10),
            clippedSamples: clipped
        )
    }

    public static func dbfs(_ amplitude: Double) -> Double {
        guard amplitude > 0 else { return floorDBFS }
        return max(floorDBFS, 20 * log10(amplitude))
    }

    /// Nearest-rank percentile; an empty list reads as silence.
    static func percentile(_ sorted: [Double], _ fraction: Double) -> Double {
        guard !sorted.isEmpty else { return floorDBFS }
        let rank = Int((fraction * Double(sorted.count)).rounded(.up))
        return sorted[min(sorted.count, max(1, rank)) - 1]
    }

    static func rounded(_ value: Double) -> Double {
        (value * 100).rounded() / 100
    }
}

/// Content-free level numbers for one recording. Window percentiles are over
/// 100 ms windows: p90 approximates the speech level, p10 the noise floor.
public struct LevelSummary: Codable, Equatable, Sendable {
    public let windowSeconds: Double
    public let samples: Int
    public let windows: Int
    public let rmsDBFS: Double
    public let peakDBFS: Double
    public let windowRMSP10DBFS: Double
    public let windowRMSP50DBFS: Double
    public let windowRMSP90DBFS: Double
    public let speechMinusNoiseDB: Double
    public let clippedSamples: Int

    public init(samples: Int, windows: Int, rmsDBFS: Double, peakDBFS: Double, windowRMSP10DBFS: Double, windowRMSP50DBFS: Double, windowRMSP90DBFS: Double, speechMinusNoiseDB: Double, clippedSamples: Int) {
        windowSeconds = LevelAccumulator.windowSeconds
        self.samples = samples
        self.windows = windows
        self.rmsDBFS = rmsDBFS
        self.peakDBFS = peakDBFS
        self.windowRMSP10DBFS = windowRMSP10DBFS
        self.windowRMSP50DBFS = windowRMSP50DBFS
        self.windowRMSP90DBFS = windowRMSP90DBFS
        self.speechMinusNoiseDB = speechMinusNoiseDB
        self.clippedSamples = clippedSamples
    }

    enum CodingKeys: String, CodingKey {
        case windowSeconds = "window_seconds"
        case samples
        case windows
        case rmsDBFS = "rms_dbfs"
        case peakDBFS = "peak_dbfs"
        case windowRMSP10DBFS = "window_rms_p10_dbfs"
        case windowRMSP50DBFS = "window_rms_p50_dbfs"
        case windowRMSP90DBFS = "window_rms_p90_dbfs"
        case speechMinusNoiseDB = "speech_minus_noise_db"
        case clippedSamples = "clipped_samples"
    }
}
