package integration

import io.quarkus.test.common.QuarkusTestResourceLifecycleManager
import org.testcontainers.containers.PostgreSQLContainer

class PostgresContainer: QuarkusTestResourceLifecycleManager {
    companion object {
        private val container = PostgreSQLContainer("postgres:16.1")
            .withDatabaseName("squirrel")
            .withUsername("test")
            .withPassword("test")
    }

    override fun start(): MutableMap<String, String> {
        container.start()

        return mutableMapOf(
            "quarkus.datasource.jdbc.url" to container.jdbcUrl,
            "quarkus.datasource.username" to "test",
            "quarkus.datasource.password" to "test"
        )
    }

    override fun stop() {
        container.stop()
    }
}