package app.squirrel.shared.ios

import io.ktor.client.engine.HttpClientEngine
import io.ktor.client.engine.darwin.Darwin

/** The app's HTTP engine: NSURLSession through Ktor. */
fun darwinEngine(): HttpClientEngine = Darwin.create()
