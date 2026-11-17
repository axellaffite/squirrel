package app.squirrel.ui.images

import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.pow
import kotlin.math.sign

/**
 * Blurhash decoding (https://blurha.sh, MIT reference algorithm), a port of the web's
 * `web/src/lib/photos/blurhash.ts`: the blurred colour preview painted while a thumbnail loads.
 */
object Blurhash {
    private const val DIGITS = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~"
    private const val MIN_LENGTH = 6
    private const val OPAQUE = 0xFF shl 24

    private fun decode83(text: String): Int {
        var value = 0
        for (char in text) {
            val digit = DIGITS.indexOf(char)
            require(digit >= 0) { "invalid blurhash" }
            value = value * 83 + digit
        }
        return value
    }

    private fun srgbToLinear(value: Int): Double {
        val v = value / 255.0
        return if (v <= 0.04045) v / 12.92 else ((v + 0.055) / 1.055).pow(2.4)
    }

    private fun linearToSrgb(value: Double): Int {
        val v = value.coerceIn(0.0, 1.0)
        return (if (v <= 0.0031308) v * 12.92 * 255 + 0.5 else (1.055 * v.pow(1 / 2.4) - 0.055) * 255 + 0.5).toInt()
    }

    private fun signPow(value: Double, exp: Double) = sign(value) * abs(value).pow(exp)

    fun isValid(hash: String?): Boolean {
        if (hash == null || hash.length < MIN_LENGTH) return false
        return try {
            val size = decode83(hash.substring(0, 1))
            val nx = size % 9 + 1
            val ny = size / 9 + 1
            hash.length == 4 + 2 * nx * ny && decode83(hash) >= 0
        } catch (_: IllegalArgumentException) {
            false
        }
    }

    /** ARGB of the average colour: instant, before any pixel is decoded. */
    fun averageColor(hash: String): Int = OPAQUE or decode83(hash.substring(2, 6))

    /** [width] × [height] ARGB pixels of the blurred preview. */
    fun decode(hash: String, width: Int, height: Int): IntArray {
        require(isValid(hash)) { "invalid blurhash" }
        val size = decode83(hash.substring(0, 1))
        val nx = size % 9 + 1
        val ny = size / 9 + 1
        val maxValue = (decode83(hash.substring(1, 2)) + 1) / 166.0
        val colors = Array(nx * ny) { i ->
            if (i == 0) {
                val value = decode83(hash.substring(2, 6))
                doubleArrayOf(srgbToLinear(value shr 16), srgbToLinear((value shr 8) and 255), srgbToLinear(value and 255))
            } else {
                val value = decode83(hash.substring(4 + i * 2, 6 + i * 2))
                val quant = intArrayOf(value / (19 * 19), (value / 19) % 19, value % 19)
                DoubleArray(3) { c -> signPow((quant[c] - 9) / 9.0, 2.0) * maxValue }
            }
        }
        val pixels = IntArray(width * height)
        for (y in 0 until height) {
            for (x in 0 until width) {
                var r = 0.0
                var g = 0.0
                var b = 0.0
                for (j in 0 until ny) {
                    for (i in 0 until nx) {
                        val basis = cos(PI * x * i / width) * cos(PI * y * j / height)
                        val color = colors[i + j * nx]
                        r += color[0] * basis
                        g += color[1] * basis
                        b += color[2] * basis
                    }
                }
                pixels[x + y * width] = OPAQUE or (linearToSrgb(r) shl 16) or (linearToSrgb(g) shl 8) or linearToSrgb(b)
            }
        }
        return pixels
    }
}

