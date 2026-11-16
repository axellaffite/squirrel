package app.squirrel.shared.api

import app.squirrel.api.models.DeduplicatedUpload
import app.squirrel.api.models.UploadSessionDto
import app.squirrel.api.models.NodeDto
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.http.ContentType
import io.ktor.http.HttpStatusCode
import io.ktor.http.contentType
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** What the uploader asks `POST /api/uploads` for. */
data class NewUpload(
    val parentId: String,
    val name: String,
    val size: Long,
    val sha256: String,
    /** RFC 3339 with the phone's UTC offset, e.g. `2024-07-14T18:30:05+02:00`. */
    val takenAt: String?,
    val deviceAssetId: String?,
)

sealed interface UploadOutcome {
    /** 200: the server already had the bytes and made a node of them. */
    data class Deduplicated(val node: NodeDto) : UploadOutcome
    /** 201: an upload session to send chunks to. */
    data class Session(val session: UploadSessionDto) : UploadOutcome
}

/**
 * The generated `CreateUploadRequest` cannot carry this body: its `taken_at` is a
 * `kotlin.time.Instant`, which serialises in UTC and drops the offset the server stores with
 * the capture time. Same schema, checked against openapi.json by `UploadContractTest`.
 */
@Serializable
internal data class CreateUploadBody(
    @SerialName("parent_id") val parentId: String,
    @SerialName("name") val name: String,
    @SerialName("size") val size: Long,
    @SerialName("sha256") val sha256: String,
    @SerialName("taken_at") val takenAt: String? = null,
    @SerialName("device_asset_id") val deviceAssetId: String? = null,
)

/** `POST /api/uploads`, whose two success statuses carry two different bodies. */
class UploadsFacade internal constructor(private val baseUrl: String, private val client: HttpClient) {
    suspend fun create(upload: NewUpload): UploadOutcome {
        val response = client.post("$baseUrl/api/uploads") {
            contentType(ContentType.Application.Json)
            setBody(
                CreateUploadBody(
                    parentId = upload.parentId,
                    name = upload.name,
                    size = upload.size,
                    sha256 = upload.sha256,
                    takenAt = upload.takenAt,
                    deviceAssetId = upload.deviceAssetId,
                ),
            )
        }.throwUnlessSuccess()
        return when (response.status) {
            HttpStatusCode.Created -> UploadOutcome.Session(response.body<UploadSessionDto>())
            else -> UploadOutcome.Deduplicated(response.body<DeduplicatedUpload>().node)
        }
    }
}
