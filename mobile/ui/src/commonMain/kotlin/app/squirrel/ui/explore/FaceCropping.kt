package app.squirrel.ui.explore

import app.squirrel.api.models.FaceBoxDto
import app.squirrel.api.models.FaceCrop
import app.squirrel.shared.api.SquirrelApi

/** Room kept around the face, as a share of its height on each side. */
const val FACE_MARGIN = 0.25

/** The thumb fits 1 080 × 360 px: 360 px tall unless wider than 3:1. */
private const val THUMB_PX = 360.0
private const val THUMB_MAX_WIDTH = 1080.0

/** The part of the photo a round avatar shows, relative (0–1) to the photo. */
data class CropWindow(val top: Double, val height: Double, val centre: Double, val width: Double)

/**
 * The window of the photo (width / height [aspect]) that a square avatar shows so the face box,
 * with [margin] around it, fills it; it never leaves the photo. Ported from `faces.ts`.
 */
fun cropWindow(box: FaceBoxDto, aspect: Double = 1.0, margin: Double = FACE_MARGIN): CropWindow {
    val boxHeight = box.height.toDouble()
    val height = (boxHeight * (1 + 2 * margin)).coerceIn(0.01, 1.0)
    val top = (box.y.toDouble() - boxHeight * margin).coerceIn(0.0, 1 - height)
    val width = (height / if (aspect > 0) aspect else 1.0).coerceIn(0.0, 1.0)
    val centre = (box.x.toDouble() + box.width.toDouble() / 2.0).coerceIn(width / 2, 1 - width / 2)
    return CropWindow(top, height, centre, width)
}

/**
 * The thumbnail to crop for an avatar [displayPx] device pixels wide: the thumb when its pixels
 * cover the avatar, else the preview (small faces). Null while the photo has no thumbnail.
 */
fun cropSource(api: SquirrelApi, face: FaceCrop, displayPx: Double, aspect: Double = 1.0): String? {
    val version = face.thumbnailVersion ?: return null
    val thumbHeight = minOf(THUMB_PX, THUMB_MAX_WIDTH / if (aspect > 0) aspect else 1.0)
    val covered = cropWindow(face.box).height * thumbHeight
    return api.thumbnailUrl(face.photoId, version, preview = covered < displayPx)
}
