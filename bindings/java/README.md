# tomet-java

High-performance Java and JVM bindings for Tomet markup and AST, powered by JNI and Rust core.

## Installation

### Gradle (Kotlin DSL)
```kotlin
dependencies {
    implementation("org.tomet:tomet:0.1.0")
}
```

### Maven
```xml
<dependency>
    <groupId>org.tomet</groupId>
    <artifactId>tomet</artifactId>
    <version>0.1.0</version>
</dependency>
```

## Quick Start

```java
import org.tomet.tomet.Tomet;

public class Main {
    public static void main(String[] args) {
        // 1. Parse markup document into AST JSON
        String docJson = Tomet.parseDocumentJson("#[ Hello Java ]\n\n<task>(done: true)[Buy milk]");
        System.out.println("AST JSON: " + docJson);

        // 2. Convert to HTML and Markdown
        String html = Tomet.toHtml("#[ Hello ]\n\nProse text");
        String md = Tomet.toMarkdown("#[ Hello ]\n\nProse text");

        // 3. Format source text
        String formatted = Tomet.format("#[  Messy  Heading  ]\n");
        System.out.println(formatted);
    }
}
```

### Kotlin Example
```kotlin
import org.tomet.tomet.Tomet

fun main() {
    val html = Tomet.toHtml("#[ Kotlin + Tomet ]\n\n<task>(done: true)[Hello!]")
    println(html)
}
```
