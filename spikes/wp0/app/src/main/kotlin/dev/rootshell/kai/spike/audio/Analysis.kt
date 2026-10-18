package dev.rootshell.kai.spike.audio

import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * A simple energy VAD over 20 ms frames, as a stand-in for a server VAD's barge-in decision: speech
 * starts after [onFrames] frames above the threshold and ends after [offFrames] frames below it.
 * Each start inside the window is one barge-in.
 */
class EnergyVad(val thresholdDb: Double, val onFrames: Int = 10, val offFrames: Int = 15) {
    data class Result(val onsets: Int, val framesAbove: Int, val frames: Int, val onsetTimesMs: List<Int>)

    /** Counts onsets in frame levels `dbs` (20 ms each) from `from` to `to` (frame indices). */
    fun run(dbs: DoubleArray, from: Int, to: Int): Result {
        var onsets = 0
        var above = 0
        var run = 0
        var quiet = 0
        var inSpeech = false
        val times = mutableListOf<Int>()
        for (i in from until minOf(to, dbs.size)) {
            if (dbs[i] > thresholdDb) {
                above++
                run++
                quiet = 0
                if (!inSpeech && run >= onFrames) {
                    inSpeech = true
                    onsets++
                    times += (i - from - onFrames + 1) * 20
                }
            } else {
                run = 0
                quiet++
                if (inSpeech && quiet >= offFrames) inSpeech = false
            }
        }
        return Result(onsets, above, max(0, minOf(to, dbs.size) - from), times)
    }

    companion object {
        const val FRAME = Pcm.RATE / 50

        fun frameDbs(x: ShortArray): DoubleArray = DoubleArray(x.size / FRAME) { Pcm.db(x, it * FRAME, FRAME) }

        fun frameDbs(x: FloatArray): DoubleArray = DoubleArray(x.size / FRAME) { Pcm.db(x, it * FRAME, FRAME) }

        fun median(v: DoubleArray, from: Int, to: Int): Double {
            val s = v.copyOfRange(from.coerceIn(0, v.size), to.coerceIn(0, v.size)).sorted()
            return if (s.isEmpty()) -120.0 else s[s.size / 2]
        }
    }
}

/**
 * The software canceller's stand-in for (d): a time-domain NLMS filter over the known reference,
 * after a coarse delay search. Not WebRTC's AEC3 (which needs the reference through libwebrtc's own
 * playout); it shows how much a plain adaptive filter removes from the raw signal.
 */
object Nlms {
    /**
     * The delay (in samples, 0 to `maxLag`) at which `ref` best matches `mic`: on 4 ms energy
     * envelopes first, then sample by sample around the envelopes' best lag.
     */
    fun delay(mic: FloatArray, ref: FloatArray, maxLag: Int): Int {
        val block = 64
        fun env(x: FloatArray) = FloatArray(x.size / block) { b ->
            var s = 0f
            for (i in b * block until (b + 1) * block) s += x[i] * x[i]
            sqrt(s)
        }
        val m = env(mic)
        val r = env(ref)
        var best = 0
        var bestScore = Double.NEGATIVE_INFINITY
        for (lag in 0..maxLag / block) {
            var s = 0.0
            for (i in r.indices) {
                val j = i + lag
                if (j >= m.size) break
                s += r[i].toDouble() * m[j]
            }
            if (s > bestScore) {
                bestScore = s
                best = lag
            }
        }
        // Then sample by sample around it, on the samples' own cross-correlation (2 s of it).
        val coarse = best * block
        val span = minOf(ref.size, 2 * Pcm.RATE)
        var fine = coarse
        var fineScore = Double.NEGATIVE_INFINITY
        for (lag in maxOf(0, coarse - 2 * block)..minOf(maxLag, coarse + 2 * block)) {
            var s = 0.0
            for (i in 0 until span) {
                val j = i + lag
                if (j >= mic.size) break
                s += ref[i].toDouble() * mic[j]
            }
            if (s > fineScore) {
                fineScore = s
                fine = lag
            }
        }
        return fine
    }

    /**
     * `mic` with the echo of `ref` removed: an NLMS filter of `taps` taps, `ref` delayed by
     * `delay` samples (the filter's start; keep a few ms before the delay [delay] found).
     */
    fun cancel(mic: FloatArray, ref: FloatArray, delay: Int, taps: Int = 1024, mu: Float = 0.5f): FloatArray {
        val x = FloatArray(mic.size + taps)
        // x[taps + n] is the reference at mic sample n, shifted by the delay.
        for (n in mic.indices) {
            val k = n - delay
            if (k in ref.indices) x[taps + n] = ref[k]
        }
        val w = FloatArray(taps)
        val out = FloatArray(mic.size)
        var energy = 0f
        for (n in mic.indices) {
            val base = taps + n
            val newest = x[base]
            val oldest = x[base - taps]
            energy += newest * newest - oldest * oldest
            if (energy < 0f) energy = 0f
            var y = 0f
            for (k in 0 until taps) y += w[k] * x[base - k]
            val e = mic[n] - y
            out[n] = e
            val g = mu * e / (energy + 1e-4f)
            if (g != 0f) for (k in 0 until taps) w[k] += g * x[base - k]
        }
        return out
    }
}

/** Where a recording's energy lies: the share above a frequency, from 512-point FFTs. */
object Spectrum {
    /** The share of energy above `hz` in the frames louder than `minDb` (16 kHz input). */
    fun shareAbove(x: ShortArray, hz: Double, minDb: Double): Double? {
        val n = 512
        var total = 0.0
        var high = 0.0
        val cut = (hz / Pcm.RATE * n).toInt()
        val re = DoubleArray(n)
        val im = DoubleArray(n)
        var frames = 0
        var i = 0
        while (i + n <= x.size) {
            if (Pcm.db(x, i, n) > minDb) {
                for (k in 0 until n) {
                    val win = 0.5 - 0.5 * cos(2 * PI * k / (n - 1))
                    re[k] = x[i + k] * win
                    im[k] = 0.0
                }
                fft(re, im)
                for (k in 1 until n / 2) {
                    val p = re[k] * re[k] + im[k] * im[k]
                    total += p
                    if (k >= cut) high += p
                }
                frames++
            }
            i += n
        }
        return if (frames == 0 || total == 0.0) null else high / total
    }

    /** In-place radix-2 FFT. */
    fun fft(re: DoubleArray, im: DoubleArray) {
        val n = re.size
        var j = 0
        for (i in 1 until n) {
            var bit = n shr 1
            while (j and bit != 0) {
                j = j xor bit
                bit = bit shr 1
            }
            j = j xor bit
            if (i < j) {
                var t = re[i]; re[i] = re[j]; re[j] = t
                t = im[i]; im[i] = im[j]; im[j] = t
            }
        }
        var len = 2
        while (len <= n) {
            val ang = -2 * PI / len
            for (s in 0 until n step len) {
                for (k in 0 until len / 2) {
                    val c = cos(ang * k)
                    val sn = sin(ang * k)
                    val ar = re[s + k + len / 2] * c - im[s + k + len / 2] * sn
                    val ai = re[s + k + len / 2] * sn + im[s + k + len / 2] * c
                    re[s + k + len / 2] = re[s + k] - ar
                    im[s + k + len / 2] = im[s + k] - ai
                    re[s + k] += ar
                    im[s + k] += ai
                }
            }
            len = len shl 1
        }
    }
}
