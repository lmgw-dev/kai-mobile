package dev.rootshell.kai.spike.probe

import java.io.File
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * (e) on the phone: every native library in the installed APK, read from the zip's own records —
 * stored or compressed, whether its data starts on a 16 KB boundary, and its ELF LOAD segments'
 * smallest p_align (design §2.1: both must be 16 KB for 16 KB-page devices).
 */
object ApkCheck {
    data class Lib(
        val name: String,
        val size: Long,
        val stored: Boolean,
        val dataOffset: Long,
        val minLoadAlign: Long?,
    ) {
        val zipAligned16k: Boolean get() = stored && dataOffset % 16384 == 0L
        val elfAligned16k: Boolean get() = (minLoadAlign ?: 0) >= 16384
        val ok: Boolean get() = zipAligned16k && elfAligned16k

        fun toMap(): Map<String, Any?> = mapOf(
            "name" to name, "size" to size, "stored" to stored, "data_offset" to dataOffset,
            "zip_16k" to zipAligned16k, "min_load_align" to minLoadAlign, "elf_16k" to elfAligned16k,
        )
    }

    /** The libraries under `lib/` in `apk`. */
    fun libs(apk: File): List<Lib> = RandomAccessFile(apk, "r").use { raf ->
        val len = raf.length()
        val tailLen = minOf(len, 65_557L).toInt()
        val tail = ByteArray(tailLen)
        raf.seek(len - tailLen)
        raf.readFully(tail)
        val tb = ByteBuffer.wrap(tail).order(ByteOrder.LITTLE_ENDIAN)
        var eocd = -1
        for (i in tailLen - 22 downTo 0) {
            if (tb.getInt(i) == 0x06054b50) {
                eocd = i
                break
            }
        }
        require(eocd >= 0) { "no end of central directory in ${apk.name}" }
        val entries = tb.getShort(eocd + 10).toInt() and 0xffff
        val cdSize = tb.getInt(eocd + 12).toLong() and 0xffffffffL
        val cdOffset = tb.getInt(eocd + 16).toLong() and 0xffffffffL
        val cd = ByteArray(cdSize.toInt())
        raf.seek(cdOffset)
        raf.readFully(cd)
        val b = ByteBuffer.wrap(cd).order(ByteOrder.LITTLE_ENDIAN)
        val out = mutableListOf<Lib>()
        var p = 0
        repeat(entries) {
            require(b.getInt(p) == 0x02014b50) { "bad central directory entry at $p" }
            val method = b.getShort(p + 10).toInt() and 0xffff
            val compressed = b.getInt(p + 20).toLong() and 0xffffffffL
            val nameLen = b.getShort(p + 28).toInt() and 0xffff
            val extraLen = b.getShort(p + 30).toInt() and 0xffff
            val commentLen = b.getShort(p + 32).toInt() and 0xffff
            val local = b.getInt(p + 42).toLong() and 0xffffffffL
            val name = String(cd, p + 46, nameLen)
            if (name.startsWith("lib/") && name.endsWith(".so")) {
                val lh = ByteArray(30)
                raf.seek(local)
                raf.readFully(lh)
                val lb = ByteBuffer.wrap(lh).order(ByteOrder.LITTLE_ENDIAN)
                val data = local + 30 + (lb.getShort(26).toInt() and 0xffff) + (lb.getShort(28).toInt() and 0xffff)
                val align = if (method == 0) minLoadAlign(raf, data) else null
                out += Lib(name, compressed, method == 0, data, align)
            }
            p += 46 + nameLen + extraLen + commentLen
        }
        out
    }

    /** The smallest p_align of the PT_LOAD segments of the 64-bit ELF at `at`. */
    private fun minLoadAlign(raf: RandomAccessFile, at: Long): Long? {
        val h = ByteArray(64)
        raf.seek(at)
        raf.readFully(h)
        if (h[0].toInt() != 0x7f || h[1].toInt().toChar() != 'E' || h[4].toInt() != 2) return null
        val hb = ByteBuffer.wrap(h).order(ByteOrder.LITTLE_ENDIAN)
        val phoff = hb.getLong(0x20)
        val phentsize = hb.getShort(0x36).toInt() and 0xffff
        val phnum = hb.getShort(0x38).toInt() and 0xffff
        val ph = ByteArray(phentsize * phnum)
        raf.seek(at + phoff)
        raf.readFully(ph)
        val pb = ByteBuffer.wrap(ph).order(ByteOrder.LITTLE_ENDIAN)
        var min: Long? = null
        for (i in 0 until phnum) {
            val o = i * phentsize
            if (pb.getInt(o) == 1) {
                val align = pb.getLong(o + 0x30)
                min = if (min == null) align else minOf(min, align)
            }
        }
        return min
    }
}
