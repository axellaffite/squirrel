package app.squirrel.shared.notifications

import app.squirrel.api.models.NotificationDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.settings.SettingsStore
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlin.time.Instant

/** At most this many separate OS notifications per run; more become one summary. */
const val MAX_SEPARATE = 3

/** One page is enough: anything older than the 30 newest was announced or is stale. */
private const val FETCH_LIMIT = 30

/** The newest notification announced ([at]), and the ids announced at that very instant. */
@Serializable
data class AnnounceMark(val at: Instant, val ids: Set<String>)

/** Unread items not announced yet, newest first (as the server lists them). */
fun unannounced(items: List<NotificationDto>, mark: AnnounceMark?): List<NotificationDto> = items.filter { item ->
    !item.read && (mark == null || item.createdAt > mark.at || (item.createdAt == mark.at && item.id !in mark.ids))
}

/** The mark once [announced] (not empty) were announced too. */
fun advance(mark: AnnounceMark?, announced: List<NotificationDto>): AnnounceMark {
    val newest = announced.maxOf { it.createdAt }
    val atNewest = announced.filter { it.createdAt == newest }.map { it.id }.toSet()
    return when {
        mark == null || newest > mark.at -> AnnounceMark(newest, atNewest)
        newest == mark.at -> AnnounceMark(newest, mark.ids + atNewest)
        else -> mark
    }
}

/** What to post for [fresh] (newest first): one each, oldest first so the newest lands on top; or one summary. */
fun announcements(fresh: List<NotificationDto>, max: Int = MAX_SEPARATE): List<OsNotification> =
    if (fresh.size <= max) fresh.reversed().map(::osNotification) else listOf(summaryNotification(fresh))

/** Posts OS notifications; one per platform. */
interface NotificationPoster {
    /** False when the user refused (or not yet allowed) notifications: nothing is announced then. */
    suspend fun enabled(): Boolean

    fun post(notifications: List<OsNotification>)
}

/** Where the mark is kept between runs. */
interface AnnounceStore {
    suspend fun load(): AnnounceMark?
    suspend fun save(mark: AnnounceMark)
}

private const val MARK_KEY = "notifications.announced"

/** The mark in the settings table; sign-out forgets it with the rest of the account's state. */
class SettingsAnnounceStore(private val settings: SettingsStore) : AnnounceStore {
    override suspend fun load(): AnnounceMark? = settings.value(MARK_KEY)?.let {
        try {
            Json.decodeFromString(AnnounceMark.serializer(), it)
        } catch (_: IllegalArgumentException) {
            null
        }
    }

    override suspend fun save(mark: AnnounceMark) = settings.setValue(MARK_KEY, Json.encodeToString(AnnounceMark.serializer(), mark))
}

/**
 * After a background run: posts an OS notification for each unread item not announced before
 * (or one summary when there are many), then remembers the newest announced.
 */
class NotificationAnnouncer(
    private val fetch: suspend () -> List<NotificationDto>,
    private val store: AnnounceStore,
    private val poster: NotificationPoster,
    private val max: Int = MAX_SEPARATE,
) {
    constructor(api: SquirrelApi, store: AnnounceStore, poster: NotificationPoster) : this(
        fetch = { api.notifications.listNotifications(limit = FETCH_LIMIT).bodyOrThrow().items },
        store = store,
        poster = poster,
    )

    /** The number of items announced. */
    suspend fun announce(): Int {
        if (!poster.enabled()) return 0
        val items = fetch()
        val mark = store.load()
        val fresh = unannounced(items, mark)
        if (fresh.isEmpty()) return 0
        poster.post(announcements(fresh, max))
        store.save(advance(mark, fresh))
        return fresh.size
    }
}
