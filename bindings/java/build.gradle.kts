plugins {
    `java-library`
    `maven-publish`
}

group = "org.tomet"
version = "0.1.0"

java {
    toolchain {
        languageVersion.set(JavaLanguageVersion.of(17))
    }
    withSourcesJar()
    withJavadocJar()
}

publishing {
    publications {
        create<MavenPublication>("mavenJava") {
            from(components["java"])
            pom {
                name.set("Tomet Java Bindings")
                description.set("High-performance Java and JVM bindings for Tomet markup and AST parser")
                url.set("https://github.com/tomet/tomet")
            }
        }
    }
}
