package dev.rootshell.kai.spike.probe

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyInfo
import android.security.keystore.KeyProperties
import dev.rootshell.kai.spike.ffi.KeystoreException
import java.security.KeyFactory
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.Signature
import java.security.spec.ECGenParameterSpec
import java.util.concurrent.ConcurrentHashMap
import dev.rootshell.kai.spike.ffi.Keystore as KeystoreCallback

/**
 * The `Keystore` callback of design §3.2 over the Android Keystore (§4.3): a P-256 signing key,
 * TEE-backed, non-extractable, no user authentication, not StrongBox; `SHA256withECDSA` gives the
 * DER signature TLS 1.3 wants. The key handle is cached per alias, as the real one will be.
 */
class AndroidKeystore : KeystoreCallback {
    private val store: KeyStore by lazy { KeyStore.getInstance("AndroidKeyStore").apply { load(null) } }
    private val keys = ConcurrentHashMap<String, PrivateKey>()

    /** The security level of the last generated key (KeyProperties.SECURITY_LEVEL_*). */
    @Volatile var lastSecurityLevel: Int? = null
        private set

    override fun generate(alias: String): ByteArray = wrap("generate") {
        val kpg = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, "AndroidKeyStore")
        kpg.initialize(
            KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_SIGN)
                .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                .setDigests(KeyProperties.DIGEST_SHA256)
                .build(),
        )
        val pair = kpg.generateKeyPair()
        keys[alias] = pair.private
        lastSecurityLevel = runCatching {
            KeyFactory.getInstance(pair.private.algorithm, "AndroidKeyStore")
                .getKeySpec(pair.private, KeyInfo::class.java).securityLevel
        }.getOrNull()
        pair.public.encoded
    }

    override fun sign(alias: String, message: ByteArray): ByteArray = wrap("sign") {
        val key = keys.getOrPut(alias) { store.getKey(alias, null) as PrivateKey }
        Signature.getInstance("SHA256withECDSA").run {
            initSign(key)
            update(message)
            sign()
        }
    }

    override fun delete(alias: String) {
        keys.remove(alias)
        runCatching { store.deleteEntry(alias) }
    }

    private fun <T> wrap(what: String, f: () -> T): T = try {
        f()
    } catch (e: Exception) {
        throw KeystoreException.Failed("$what: ${e.javaClass.simpleName}: ${e.message}")
    }

    companion object {
        fun levelName(level: Int?): String = when (level) {
            null -> "unknown"
            KeyProperties.SECURITY_LEVEL_SOFTWARE -> "software"
            KeyProperties.SECURITY_LEVEL_TRUSTED_ENVIRONMENT -> "TEE"
            KeyProperties.SECURITY_LEVEL_STRONGBOX -> "StrongBox"
            else -> "level $level"
        }
    }
}
