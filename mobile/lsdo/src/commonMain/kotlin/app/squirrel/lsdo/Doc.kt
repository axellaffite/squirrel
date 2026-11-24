package app.squirrel.lsdo

import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.SerialInfo

/** What an action, a parameter or a state field means, shown to the agent next to it. */
@OptIn(ExperimentalSerializationApi::class)
@SerialInfo
@Target(AnnotationTarget.CLASS, AnnotationTarget.PROPERTY)
annotation class Doc(val text: String)
