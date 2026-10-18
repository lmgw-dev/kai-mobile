package dev.rootshell.kai.core

import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyInfo
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import dev.rootshell.kai.core.ffi.Keystore
import dev.rootshell.kai.core.ffi.KeystoreException
import java.security.KeyFactory
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.Signature
import java.security.UnrecoverableKeyException
import java.security.spec.ECGenParameterSpec
import java.util.concurrent.ConcurrentHashMap

/**
 * The device key (design §3.2, §4.3): a P-256 signing key in the Android Keystore under the alias
 * the core names, TEE-backed and non-extractable; no user authentication and no
 * `setUnlockedDeviceRequired` (the held feed connects while the phone is locked); not StrongBox (a
 * signature sits on every new connection's path). `PublicKey.getEncoded()` is the DER
 * SubjectPublicKeyInfo enrolment sends; `SHA256withECDSA` gives the DER signature TLS 1.3 wants.
 * The core calls it from its tokio threads; the private key never leaves the Keystore.
 *
 * A failure says whether the key is gone for good (`KeystoreException.Gone`: invalidated,
 * unrecoverable, no entry), which stops the link with "pair again", or failed otherwise
 * (`KeystoreException.Failed`), which the core tries again on its backoff.
 */
class AndroidKeystore : Keystore {
    private val store: KeyStore by lazy { KeyStore.getInstance(PROVIDER).apply { load(null) } }
    private val keys = ConcurrentHashMap<String, PrivateKey>()

    override fun generate(alias: String): ByteArray = guarded("generate") {
        val kpg = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, PROVIDER)
        kpg.initialize(
            KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_SIGN)
                .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                .setDigests(KeyProperties.DIGEST_SHA256)
                .build(),
        )
        val pair = kpg.generateKeyPair()
        keys[alias] = pair.private
        pair.public.encoded
    }

    override fun sign(alias: String, message: ByteArray): ByteArray = guarded("sign") {
        val key = keys[alias] ?: (store.getKey(alias, null) as? PrivateKey)?.also { keys[alias] = it }
            ?: throw NoEntry(alias)
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

    override fun aliases(): List<String> = guarded("aliases") {
        store.aliases().toList().filter { it.startsWith(KEY_PREFIX) }
    }

    private fun <T> guarded(what: String, f: () -> T): T = try {
        f()
    } catch (e: Exception) {
        val reason = "$what: ${e.javaClass.simpleName}: ${e.message}"
        throw if (gone(e)) KeystoreException.Gone(reason) else KeystoreException.Failed(reason)
    }

    /** No key under the alias: gone, like an invalidated or unrecoverable one. */
    private class NoEntry(alias: String) : Exception("no key under $alias in the Android Keystore")

    companion object {
        private const val PROVIDER = "AndroidKeyStore"

        /** The core's `KEY_PREFIX`: every device key's alias starts with it. */
        private const val KEY_PREFIX = "kai-device-"

        /**
         * The security level of the key under `alias` (`KeyProperties.SECURITY_LEVEL_*`), or null
         * when it cannot be read: the gateway's settings page warns when it is neither TEE nor
         * StrongBox (design §4.3), and never refuses the key.
         */
        fun securityLevel(alias: String): Int? = runCatching {
            val store = KeyStore.getInstance(PROVIDER).apply { load(null) }
            val key = store.getKey(alias, null) as? PrivateKey ?: return@runCatching null
            KeyFactory.getInstance(key.algorithm, PROVIDER).getKeySpec(key, KeyInfo::class.java).securityLevel
        }.getOrNull()

        /** Whether `level` is secure hardware Kai trusts: the TEE or StrongBox. */
        fun inSecureHardware(level: Int?): Boolean =
            level == KeyProperties.SECURITY_LEVEL_TRUSTED_ENVIRONMENT || level == KeyProperties.SECURITY_LEVEL_STRONGBOX

        /** Whether `e` (or a cause) says the key is gone for good, not that this one try failed. */
        private fun gone(e: Throwable): Boolean = generateSequence(e) { it.cause }.any { c ->
            c is KeyPermanentlyInvalidatedException ||
                c is UnrecoverableKeyException ||
                c is NoEntry ||
                (Build.VERSION.SDK_INT >= 33 && c is android.security.KeyStoreException &&
                    c.numericErrorCode == android.security.KeyStoreException.ERROR_KEY_DOES_NOT_EXIST)
        }
    }
}
