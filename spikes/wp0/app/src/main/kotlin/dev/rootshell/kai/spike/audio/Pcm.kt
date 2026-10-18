package dev.rootshell.kai.spike.audio

import java.io.File
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.exp
import kotlin.math.log10
import kotlin.math.max
import kotlin.math.sin
import kotlin.math.sqrt
import kotlin.random.Random

/** 16-bit mono PCM helpers: WAV files, levels, resampling. */
object Pcm {
    const val RATE = 16000

    /** Writes `samples` as a 16-bit mono WAV at `rate`. */
    fun writeWav(file: File, samples: ShortArray, rate: Int = RATE) {
        file.parentFile?.mkdirs()
        val data = samples.size * 2
        val b = ByteBuffer.allocate(44 + data).order(ByteOrder.LITTLE_ENDIAN)
        b.put("RIFF".toByteArray()).putInt(36 + data).put("WAVE".toByteArray())
        b.put("fmt ".toByteArray()).putInt(16).putShort(1).putShort(1).putInt(rate).putInt(rate * 2)
            .putShort(2).putShort(16)
        b.put("data".toByteArray()).putInt(data)
        for (s in samples) b.putShort(s)
        file.writeBytes(b.array())
    }

    /** A PCM WAV (8/16-bit, any channels and rate) as 16-bit mono at its own rate. */
    fun readWav(file: File): Pair<ShortArray, Int> {
        RandomAccessFile(file, "r").use { raf ->
            val bytes = ByteArray(raf.length().toInt())
            raf.readFully(bytes)
            val b = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
            require(String(bytes, 0, 4) == "RIFF" && String(bytes, 8, 4) == "WAVE") { "not a RIFF/WAVE file" }
            var pos = 12
            var channels = 1
            var rate = RATE
            var bits = 16
            var format = 1
            while (pos + 8 <= bytes.size) {
                val id = String(bytes, pos, 4)
                val size = b.getInt(pos + 4)
                val body = pos + 8
                if (id == "fmt ") {
                    format = b.getShort(body).toInt()
                    channels = b.getShort(body + 2).toInt()
                    rate = b.getInt(body + 4)
                    bits = b.getShort(body + 14).toInt()
                } else if (id == "data") {
                    require(format == 1 || format == -2) { "WAV format $format is not PCM" }
                    require(bits == 16 || bits == 8) { "WAV with $bits bits per sample" }
                    val frameBytes = channels * bits / 8
                    val n = minOf(size, bytes.size - body) / frameBytes
                    val out = ShortArray(n)
                    for (i in 0 until n) {
                        var sum = 0
                        for (c in 0 until channels) {
                            val at = body + i * frameBytes + c * bits / 8
                            sum += if (bits == 16) b.getShort(at).toInt() else ((bytes[at].toInt() and 0xff) - 128) shl 8
                        }
                        out[i] = (sum / channels).toShort()
                    }
                    return out to rate
                }
                pos = body + size + (size and 1)
            }
            error("the WAV has no data chunk")
        }
    }

    /** Linear resampling to `to` Hz. */
    fun resample(x: ShortArray, from: Int, to: Int): ShortArray {
        if (from == to) return x
        val n = (x.size.toLong() * to / from).toInt()
        val out = ShortArray(n)
        for (i in 0 until n) {
            val p = i.toDouble() * from / to
            val j = p.toInt()
            val f = p - j
            val a = x[minOf(j, x.size - 1)]
            val c = x[minOf(j + 1, x.size - 1)]
            out[i] = (a + (c - a) * f).toInt().toShort()
        }
        return out
    }

    /** dBFS of `n` samples from `from` (full scale = 0 dB); -120 for silence. */
    fun db(x: ShortArray, from: Int, n: Int): Double {
        var s = 0.0
        val end = minOf(x.size, from + n)
        for (i in from until end) s += x[i].toDouble() * x[i]
        val count = end - from
        if (count <= 0) return -120.0
        return 20 * log10(max(sqrt(s / count), 1e-6) / 32768.0).coerceAtLeast(-120.0)
    }

    fun db(x: FloatArray, from: Int, n: Int): Double {
        var s = 0.0
        val end = minOf(x.size, from + n)
        for (i in from until end) s += x[i].toDouble() * x[i]
        val count = end - from
        if (count <= 0) return -120.0
        return 20 * log10(max(sqrt(s / count), 1e-9)).coerceAtLeast(-120.0)
    }

    fun toFloat(x: ShortArray) = FloatArray(x.size) { x[it] / 32768f }

    fun toShort(x: FloatArray) = ShortArray(x.size) { (x[it] * 32768f).toInt().coerceIn(-32768, 32767).toShort() }
}

/**
 * The speech clip (d) plays: a recorded WAV when one was pushed to `wp0/clip.wav` (never in
 * the repository), else a generated stand-in with speech's rhythm and spectrum: syllables of a
 * glottal pulse train through three vowel formants, consonant noise bursts, pauses between words
 * and sentences.
 */
object Clip {
    data class Loaded(val samples: ShortArray, val name: String)

    fun load(dir: File): Loaded {
        val file = File(dir, "clip.wav")
        if (file.isFile) {
            val (pcm, rate) = Pcm.readWav(file)
            return Loaded(Pcm.resample(pcm, rate, Pcm.RATE), "clip.wav (${pcm.size / rate.toDouble()} s at $rate Hz)")
        }
        return Loaded(generate(15.0), "generated speech stand-in (15 s)")
    }

    /** `seconds` of the stand-in at 16 kHz, peak about -1 dBFS; the same every time. */
    fun generate(seconds: Double): ShortArray {
        val rate = Pcm.RATE
        val out = FloatArray((seconds * rate).toInt())
        val rnd = Random(20261014)
        // F1, F2, F3 of a, e, i, o, u (Hz).
        val vowels = arrayOf(
            doubleArrayOf(730.0, 1090.0, 2440.0), doubleArrayOf(530.0, 1840.0, 2480.0),
            doubleArrayOf(270.0, 2290.0, 3010.0), doubleArrayOf(570.0, 840.0, 2410.0),
            doubleArrayOf(300.0, 870.0, 2240.0),
        )
        var t = (0.2 * rate).toInt()
        var wordLeft = 0
        var sentenceLeft = 6 + rnd.nextInt(6)
        while (t < out.size) {
            if (wordLeft == 0) {
                wordLeft = 1 + rnd.nextInt(3)
                sentenceLeft--
                t += if (sentenceLeft <= 0) {
                    sentenceLeft = 6 + rnd.nextInt(6)
                    (rate * (0.5 + rnd.nextDouble() * 0.5)).toInt()
                } else {
                    (rate * (0.05 + rnd.nextDouble() * 0.2)).toInt()
                }
                continue
            }
            // A consonant burst, then the vowel.
            val burst = (rate * (0.02 + rnd.nextDouble() * 0.05)).toInt()
            var prev = 0.0
            for (i in 0 until burst) {
                if (t + i >= out.size) break
                val n = rnd.nextDouble() * 2 - 1
                val hp = n - prev
                prev = n
                out[t + i] += (0.15 * hp * sin(PI * i / burst)).toFloat()
            }
            t += burst
            val len = (rate * (0.12 + rnd.nextDouble() * 0.18)).toInt()
            val f = vowels[rnd.nextInt(vowels.size)]
            val f0a = 100 + rnd.nextDouble() * 80
            val f0b = f0a * (0.85 + rnd.nextDouble() * 0.3)
            val st = Array(3) { DoubleArray(2) }
            var phase = 0.0
            for (i in 0 until len) {
                if (t + i >= out.size) break
                val f0 = f0a + (f0b - f0a) * i / len
                phase += f0 / rate
                val pulse = if (phase >= 1.0) {
                    phase -= 1.0
                    1.0
                } else {
                    0.0
                }
                var y = 0.0
                for (k in 0 until 3) {
                    // A two-pole resonator at the formant, bandwidth ~80 Hz.
                    val r = exp(-PI * 80.0 / rate)
                    val c = 2 * r * kotlin.math.cos(2 * PI * f[k] / rate)
                    val v = pulse + c * st[k][0] - r * r * st[k][1]
                    st[k][1] = st[k][0]
                    st[k][0] = v
                    y += v / (k + 1)
                }
                val env = sin(PI * i / len)
                out[t + i] += (0.05 * y * env).toFloat()
            }
            t += len
            wordLeft--
        }
        var peak = 1e-9f
        for (v in out) peak = max(peak, abs(v))
        val gain = 0.89f / peak
        return ShortArray(out.size) { (out[it] * gain * 32767).toInt().coerceIn(-32768, 32767).toShort() }
    }
}
