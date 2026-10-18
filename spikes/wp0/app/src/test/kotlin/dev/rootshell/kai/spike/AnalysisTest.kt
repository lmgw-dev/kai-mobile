package dev.rootshell.kai.spike

import dev.rootshell.kai.spike.audio.Clip
import dev.rootshell.kai.spike.audio.EnergyVad
import dev.rootshell.kai.spike.audio.Nlms
import dev.rootshell.kai.spike.audio.Pcm
import dev.rootshell.kai.spike.audio.Spectrum
import dev.rootshell.kai.spike.probe.ApkCheck
import java.io.File
import java.util.zip.CRC32
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import kotlin.math.PI
import kotlin.math.sin
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AnalysisTest {
    @Test
    fun the_vad_counts_one_onset_per_burst_and_ignores_short_blips() {
        val dbs = DoubleArray(200) { -70.0 }
        for (i in 20 until 40) dbs[i] = -20.0 // 400 ms: an onset
        for (i in 60 until 64) dbs[i] = -20.0 // 80 ms: too short
        for (i in 100 until 130) dbs[i] = -20.0 // another onset
        val r = EnergyVad(-50.0).run(dbs, 0, dbs.size)
        assertEquals(2, r.onsets)
        assertEquals(listOf(400, 2000), r.onsetTimesMs)
    }

    @Test
    fun nlms_finds_the_delay_and_removes_most_of_a_pure_echo() {
        val ref = Pcm.toFloat(Clip.generate(4.0))
        val delay = 800 // 50 ms
        val mic = FloatArray(ref.size) { i -> if (i >= delay) 0.4f * ref[i - delay] else 0f }
        val found = Nlms.delay(mic, ref, 16000 * 3 / 10)
        assertTrue("delay $found", kotlin.math.abs(found - delay) <= 16)
        val out = Nlms.cancel(mic, ref, maxOf(0, found - 128), taps = 512)
        // After the filter has converged (the last second), the residual is far below the echo.
        val tail = mic.size - 16000
        val before = Pcm.db(mic, tail, 16000)
        val after = Pcm.db(out, tail, 16000)
        assertTrue("echo $before dB, residual $after dB", before - after > 20)
    }

    @Test
    fun the_spectrum_tells_narrowband_from_wideband() {
        val n = 16000
        val low = ShortArray(n) { (8000 * sin(2 * PI * 1000 * it / 16000.0)).toInt().toShort() }
        val high = ShortArray(n) { (8000 * sin(2 * PI * 6000 * it / 16000.0)).toInt().toShort() }
        assertTrue(Spectrum.shareAbove(low, 4000.0, -60.0)!! < 0.02)
        assertTrue(Spectrum.shareAbove(high, 4000.0, -60.0)!! > 0.9)
    }

    @Test
    fun a_wav_reads_back_as_written() {
        val f = File.createTempFile("spike", ".wav")
        try {
            val x = ShortArray(1000) { (it * 31).toShort() }
            Pcm.writeWav(f, x, 8000)
            val (y, rate) = Pcm.readWav(f)
            assertEquals(8000, rate)
            assertEquals(x.toList(), y.toList())
            assertEquals(2000, Pcm.resample(y, 8000, 16000).size)
        } finally {
            f.delete()
        }
    }

    @Test
    fun the_apk_check_reads_stored_libraries_and_their_alignment() {
        val f = File.createTempFile("spike", ".apk")
        try {
            // A minimal 64-bit ELF header with one PT_LOAD whose p_align is 0x4000.
            val elf = ByteArray(64 + 56)
            elf[0] = 0x7f; elf[1] = 'E'.code.toByte(); elf[2] = 'L'.code.toByte(); elf[3] = 'F'.code.toByte(); elf[4] = 2
            fun putLong(at: Int, v: Long) { for (i in 0 until 8) elf[at + i] = (v shr (8 * i)).toByte() }
            fun putShort(at: Int, v: Int) { elf[at] = v.toByte(); elf[at + 1] = (v shr 8).toByte() }
            putLong(0x20, 64)
            putShort(0x36, 56)
            putShort(0x38, 1)
            elf[64] = 1
            putLong(64 + 0x30, 0x4000)
            ZipOutputStream(f.outputStream()).use { z ->
                z.putNextEntry(ZipEntry("classes.dex"))
                z.write(ByteArray(10))
                z.closeEntry()
                val e = ZipEntry("lib/arm64-v8a/libx.so").apply {
                    method = ZipEntry.STORED
                    size = elf.size.toLong()
                    crc = CRC32().apply { update(elf) }.value
                }
                z.putNextEntry(e)
                z.write(elf)
                z.closeEntry()
            }
            val libs = ApkCheck.libs(f)
            assertEquals(1, libs.size)
            assertEquals("lib/arm64-v8a/libx.so", libs[0].name)
            assertTrue(libs[0].stored)
            assertEquals(0x4000L, libs[0].minLoadAlign)
            assertTrue(libs[0].elfAligned16k)
        } finally {
            f.delete()
        }
    }
}
