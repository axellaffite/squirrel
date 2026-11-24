package app.squirrel.lsdo

import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.KSerializer
import kotlinx.serialization.serializer

/**
 * A screen an agent can drive: it lists its [state] and the intents it accepts (ls), then runs
 * one (do). [I] is a `@Serializable` sealed type, one subclass per action; its serial names
 * (`@SerialName`) are the action names and its properties their parameters.
 */
interface LsDoScreen<S : Any, I : Any> {
    val lsdo: LsDoSpec<S, I>
    val state: StateFlow<S>

    /** Runs [intent] through the same code the UI calls; a refusal message, or null. */
    suspend fun perform(intent: I): String?

    /** The action names that make sense in [state]; null: all of them. */
    fun availableIntents(state: S): Set<String>? = null

    /** Work is still in flight (a load, a request): a do waits for it before answering. */
    fun isBusy(state: S): Boolean = false
}

/** The serializers of a screen's state and intents: they give the JSON and the schemas. */
class LsDoSpec<S : Any, I : Any>(val state: KSerializer<S>, val intent: KSerializer<I>)

inline fun <reified S : Any, reified I : Any> lsdoSpec(): LsDoSpec<S, I> = LsDoSpec(serializer<S>(), serializer<I>())
