package lol.cutout

import android.app.Activity
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.SystemClock
import net.openid.appauth.AuthState
import net.openid.appauth.AuthorizationException
import net.openid.appauth.AuthorizationRequest
import net.openid.appauth.AuthorizationResponse
import net.openid.appauth.AuthorizationService
import net.openid.appauth.AuthorizationServiceConfiguration
import net.openid.appauth.ResponseTypeValues
import net.openid.appauth.TokenResponse
import java.io.ByteArrayOutputStream
import java.io.Closeable
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.Executor
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong

internal data class SpotifyHttpResponse(val status: Int, val retryAfter: String?, val body: ByteArray)

/** AppAuth PKCE account and bounded Web API transport. Rust supplies and validates every URL. */
internal class AndroidSpotifyWebApi(
    context: Context,
    private val clientId: String,
    private val store: AndroidSpotifyCredentialStore = AndroidSpotifyCredentialStore.fromContext(context),
    private val request: (String, String, String, (Result<SpotifyHttpResponse>) -> Unit) -> Unit = ::httpRequest,
    private val credentialExecutor: Executor = SpotifyHttpExecutor,
) : Closeable {
    private val appContext = context.applicationContext
    private val redirectUri = Uri.parse(REDIRECT_URI)
    private val serviceConfiguration = AuthorizationServiceConfiguration(
        Uri.parse(AUTHORIZATION_ENDPOINT), Uri.parse(TOKEN_ENDPOINT),
    )
    private val serviceDelegate = lazy(LazyThreadSafetyMode.SYNCHRONIZED) { AuthorizationService(appContext) }
    private val service by serviceDelegate
    private val authLock = Any()
    private data class PendingAuthorization(val state: String, val revision: Long)
    private val pendingStates = mutableMapOf<Long, PendingAuthorization>()
    private val authRevision = AtomicLong(0)
    private val providerWorkRevision = AtomicLong(0)
    @Volatile private var credentialRestoreFailed = false
    private var authState: AuthState? = loadState()
    @Volatile private var closed = false

    val isConfigured: Boolean get() = clientId.isNotBlank()
    val isAuthorized: Boolean get() = synchronized(authLock) { authState?.isAuthorized == true }
    val hasCredentialRestoreFailure: Boolean get() = credentialRestoreFailed
    val hasPlaybackScopes: Boolean get() = synchronized(authLock) {
        val scopes = authState?.scopeSet.orEmpty()
        SCOPES.all(scopes::contains)
    }

    /** Explicitly retries reading the existing encrypted grant; it never starts OAuth. */
    fun retryCredentialRestore(completed: (Result<Boolean>) -> Unit) {
        val revision = synchronized(authLock) { authRevision.incrementAndGet() }
        try {
            credentialExecutor.execute {
                val result = synchronized(authLock) {
                    runCatching {
                        check(!closed && revision == authRevision.get())
                        val serialized = store.load()
                        val restored = serialized?.let { AuthState.jsonDeserialize(it) }
                        authState = restored
                        credentialRestoreFailed = false
                        restored?.isAuthorized == true
                    }.onFailure {
                        if (!closed && revision == authRevision.get()) credentialRestoreFailed = true
                    }
                }
                val currentResult = synchronized(authLock) {
                    if (closed || revision != authRevision.get()) {
                        Result.failure(IllegalStateException(appContext.getString(R.string.spotify_auth_stale)))
                    } else result
                }
                completed(currentResult)
            }
        } catch (error: RuntimeException) {
            completed(Result.failure(error))
        }
    }

    /** Starts PKCE only for an explicit user action. */
    fun authorize(activity: Activity, transactionId: ULong, completed: (Result<Unit>) -> Unit) {
        if (!isConfigured) {
            completed(Result.failure(IllegalStateException(appContext.getString(R.string.spotify_not_configured))))
            return
        }
        if (closed) {
            completed(Result.failure(IllegalStateException(appContext.getString(R.string.music_provider_unavailable))))
            return
        }
        try {
            val authRequest = AuthorizationRequest.Builder(
                serviceConfiguration,
                clientId,
                ResponseTypeValues.CODE,
                redirectUri,
            ).setScope(SCOPES.joinToString(" ")).build()
            synchronized(authLock) {
                val revision = authRevision.incrementAndGet()
                pendingStates[transactionId.toLong()] = PendingAuthorization(authRequest.state.orEmpty(), revision)
            }
            val complete = callbackIntent(transactionId, cancelled = false)
            val cancel = callbackIntent(transactionId, cancelled = true)
            service.performAuthorizationRequest(authRequest, complete, cancel)
            completed(Result.success(Unit))
        } catch (error: Exception) {
            synchronized(authLock) { pendingStates.remove(transactionId.toLong()) }
            completed(Result.failure(error))
        }
    }

    /** Accepts only the current PKCE transaction and AppAuth state-validated redirect response. */
    fun completeAuthorization(intent: Intent, transactionId: ULong, completed: (Result<Unit>) -> Unit) {
        val pending = synchronized(authLock) { pendingStates.remove(transactionId.toLong()) }
        if (closed || pending == null || pending.revision != authRevision.get() ||
            intent.getLongExtra(EXTRA_TRANSACTION_ID, -1L) != transactionId.toLong()) {
            completed(Result.failure(IllegalStateException(appContext.getString(R.string.spotify_auth_stale))))
            return
        }
        val response = AuthorizationResponse.fromIntent(intent)
        val exception = AuthorizationException.fromIntent(intent)
        if (intent.getBooleanExtra(EXTRA_CANCELLED, false)) {
            completed(Result.failure(SpotifyAuthorizationCancelled()))
            return
        }
        if (response == null || response.state != pending.state || response.request.clientId != clientId ||
            response.request.redirectUri != redirectUri || response.request.codeVerifier.isNullOrBlank()
        ) {
            completed(Result.failure(IllegalStateException(appContext.getString(R.string.spotify_auth_invalid))))
            return
        }
        if (exception != null) {
            completed(Result.failure(exception))
            return
        }
        val revision = pending.revision
        service.performTokenRequest(response.createTokenExchangeRequest()) { tokenResponse: TokenResponse?, tokenError: AuthorizationException? ->
            if (closed || revision != authRevision.get()) {
                completed(Result.failure(IllegalStateException(appContext.getString(R.string.spotify_auth_stale))))
                return@performTokenRequest
            }
            if (tokenResponse == null || tokenError != null) {
                completed(Result.failure(tokenError ?: IllegalStateException(appContext.getString(R.string.spotify_auth_failed))))
                return@performTokenRequest
            }
            val state = AuthState(response, null as AuthorizationException?)
            state.update(tokenResponse, null as AuthorizationException?)
            completed(installAuthorizationState(state, revision))
        }
    }

    fun clearCredentials(completed: (Result<Unit>) -> Unit = {}) {
        val revision = synchronized(authLock) {
            val value = authRevision.incrementAndGet()
            pendingStates.clear()
            authState = null
            value
        }
        try {
            credentialExecutor.execute {
                val result = synchronized(authLock) {
                    if (revision != authRevision.get()) {
                        Result.failure(IllegalStateException(appContext.getString(R.string.spotify_auth_stale)))
                    } else {
                        runCatching { store.clear() }.onSuccess { credentialRestoreFailed = false }
                    }
                }
                completed(result)
            }
        } catch (error: RuntimeException) {
            completed(Result.failure(error))
        }
    }

    fun retireAuthorization(transactionId: ULong) {
        synchronized(authLock) {
            pendingStates.remove(transactionId.toLong())
            authRevision.incrementAndGet()
        }
    }

    /** Retires only in-flight token/HTTP work while keeping an encrypted grant for later use. */
    fun retireProviderWork() {
        providerWorkRevision.incrementAndGet()
    }

    /** Requests one Rust-authorized URL with a fresh AppAuth token. */
    fun execute(url: String, method: String, forceRefresh: Boolean = false, completed: (Result<SpotifyHttpResponse>) -> Unit) {
        if (closed) {
            completed(Result.failure(IllegalStateException(appContext.getString(R.string.music_provider_unavailable))))
            return
        }
        val parsed = runCatching { Uri.parse(url) }.getOrNull()
        if (parsed?.scheme != "https" || parsed.host != SPOTIFY_API_HOST ||
            parsed.userInfo != null || parsed.port != -1 || method !in ALLOWED_METHODS
        ) {
            completed(Result.failure(IllegalArgumentException(appContext.getString(R.string.spotify_request_invalid))))
            return
        }
        val (state, revision) = currentAuthorizationSnapshot(forceRefresh)
        if (state == null || !state.isAuthorized) {
            completed(Result.failure(IllegalStateException(appContext.getString(R.string.spotify_auth_required))))
            return
        }
        val workRevision = providerWorkRevision.get()
        state.performActionWithFreshTokens(service) { accessToken, _, authError ->
            if (closed || revision != authRevision.get()) {
                completed(Result.failure(IllegalStateException(appContext.getString(R.string.spotify_auth_stale))))
            } else if (accessToken.isNullOrBlank() || authError != null) {
                completed(Result.failure(SpotifyAuthenticationFailure()))
            } else {
                val persisted = persistRefreshedState(state, revision)
                when {
                    persisted.isFailure -> completed(Result.failure(SpotifyCredentialPersistenceFailure()))
                    closed || revision != authRevision.get() ->
                        completed(Result.failure(IllegalStateException(appContext.getString(R.string.spotify_auth_stale))))
                    workRevision != providerWorkRevision.get() ->
                        completed(Result.failure(IllegalStateException(appContext.getString(R.string.spotify_auth_stale))))
                    else -> request(url, method, accessToken, completed)
                }
            }
        }
    }

    internal fun persistRefreshedState(state: AuthState, revision: Long): Result<Unit> = synchronized(authLock) {
        if (closed || revision != authRevision.get()) {
            Result.failure(IllegalStateException(appContext.getString(R.string.spotify_auth_stale)))
        } else {
            runCatching { store.save(state.jsonSerializeString()) }
        }
    }

    internal fun installAuthorizationState(state: AuthState, revision: Long): Result<Unit> = synchronized(authLock) {
        if (closed || revision != authRevision.get()) {
            Result.failure(IllegalStateException(appContext.getString(R.string.spotify_auth_stale)))
        } else {
            runCatching { store.save(state.jsonSerializeString()) }
                .onSuccess { authState = state; credentialRestoreFailed = false }
        }
    }

    internal fun currentGrantRevision(): Long = authRevision.get()

    /** Selects the credential object and its grant identity as one atomic observation. */
    internal fun currentAuthorizationSnapshot(forceRefresh: Boolean = false): Pair<AuthState?, Long> = synchronized(authLock) {
        val selected = authState
        if (forceRefresh) selected?.setNeedsTokenRefresh(true)
        selected to authRevision.get()
    }

    private fun callbackIntent(id: ULong, cancelled: Boolean): PendingIntent {
        val intent = Intent(appContext, SpotifyAuthorizationResultActivity::class.java)
            .setAction("$AUTH_RESULT_ACTION${id}-$cancelled")
            .putExtra(EXTRA_TRANSACTION_ID, id.toLong())
            .putExtra(EXTRA_CANCELLED, cancelled)
        val flags = PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE
        return PendingIntent.getActivity(appContext, id.toInt() * 2 + if (cancelled) 1 else 0, intent, flags)
    }

    private fun loadState(): AuthState? {
        val serialized = try { store.load() } catch (_: Exception) {
            credentialRestoreFailed = true
            return null
        } ?: return null
        return try { AuthState.jsonDeserialize(serialized) } catch (_: Exception) {
            credentialRestoreFailed = true
            null
        }
    }

    override fun close() {
        synchronized(authLock) {
            closed = true
            pendingStates.clear()
            authState = null
        }
        if (serviceDelegate.isInitialized()) service.dispose()
    }

    internal class SpotifyAuthorizationCancelled : RuntimeException()
    internal class SpotifyAuthenticationFailure : RuntimeException()
    internal class SpotifyCredentialPersistenceFailure : RuntimeException()

    internal companion object {
        const val REDIRECT_URI = "cutout-spotify://spotify-login-callback"
        const val EXTRA_TRANSACTION_ID = "lol.cutout.spotify.transaction"
        const val EXTRA_CANCELLED = "lol.cutout.spotify.cancelled"
        private const val AUTH_RESULT_ACTION = "lol.cutout.spotify.AUTH_RESULT."
        private const val AUTHORIZATION_ENDPOINT = "https://accounts.spotify.com/authorize"
        private const val TOKEN_ENDPOINT = "https://accounts.spotify.com/api/token"
        private const val SPOTIFY_API_HOST = "api.spotify.com"
        private const val MAX_RESPONSE_BYTES = 1_048_576
        private val ALLOWED_METHODS = setOf("GET", "PUT", "POST")
        val SCOPES = listOf("user-read-playback-state", "user-modify-playback-state")

        private fun httpRequest(url: String, method: String, accessToken: String, completed: (Result<SpotifyHttpResponse>) -> Unit) {
            val executor = SpotifyHttpExecutor
            try {
                executor.execute {
                    val result = runCatching {
                        val connection = (URL(url).openConnection() as HttpURLConnection).apply {
                            requestMethod = method
                            connectTimeout = HTTP_TIMEOUT_MS
                            readTimeout = HTTP_TIMEOUT_MS
                            instanceFollowRedirects = false
                            setRequestProperty("Authorization", "Bearer $accessToken")
                            setRequestProperty("Accept", "application/json")
                            if (method == "PUT" || method == "POST") doOutput = true
                        }
                        try {
                            val status = connection.responseCode
                            val stream = if (status in 200..299) connection.inputStream else connection.errorStream
                            val body = stream?.use(::readBounded) ?: ByteArray(0)
                            check(body.size <= MAX_RESPONSE_BYTES) { "Spotify response exceeded the local bound" }
                            SpotifyHttpResponse(status, connection.getHeaderField("Retry-After"), body)
                        } finally {
                            connection.disconnect()
                        }
                    }
                    completed(result)
                }
            } catch (error: RuntimeException) {
                completed(Result.failure(error))
            }
        }

        private fun readBounded(input: java.io.InputStream): ByteArray {
            val output = ByteArrayOutputStream()
            val chunk = ByteArray(8192)
            while (true) {
                val count = input.read(chunk)
                if (count < 0) break
                if (output.size() + count > MAX_RESPONSE_BYTES) throw IllegalStateException("Spotify response exceeded the local bound")
                output.write(chunk, 0, count)
            }
            return output.toByteArray()
        }

        private const val HTTP_TIMEOUT_MS = 10_000
    }
}

private object SpotifyHttpExecutor : ThreadPoolExecutor(
    1, 1, 0L, TimeUnit.MILLISECONDS, ArrayBlockingQueue(16),
    { task -> Thread(task, "CutOutSpotifyWebApi").apply { isDaemon = true } },
    ThreadPoolExecutor.AbortPolicy(),
)

/** AppAuth's explicit PendingIntent result target; it never treats arbitrary intents as auth. */
class SpotifyAuthorizationResultActivity : Activity() {
    override fun onCreate(savedInstanceState: android.os.Bundle?) {
        super.onCreate(savedInstanceState)
        (application as? CutoutApplication)?.music?.completeSpotifyAuthorization(intent)
        finish()
    }
}
