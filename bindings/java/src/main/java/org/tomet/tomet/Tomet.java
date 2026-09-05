package org.tomet.tomet;

/**
 * High-performance Java / JVM bindings for Tomet markup and AST parser.
 */
public final class Tomet {
    private static volatile boolean loaded = false;

    private Tomet() {}

    /**
     * Ensures that the native library (libtomet_java.so / .dylib / .dll) is loaded.
     */
    public static void loadLibrary() {
        if (!loaded) {
            synchronized (Tomet.class) {
                if (!loaded) {
                    try {
                        System.loadLibrary("tomet_java");
                    } catch (UnsatisfiedLinkError e) {
                        throw new TometException("Failed to load native tomet_java library: " + e.getMessage(), e);
                    }
                    loaded = true;
                }
            }
        }
    }

    static {
        try {
            loadLibrary();
        } catch (Throwable ignored) {
            // Deferred load on first API call if library isn't in default java.library.path yet
        }
    }

    /**
     * Parses full .tmt markup source text into a JSON string representing the Document AST.
     *
     * @param source the Tomet source text
     * @return JSON string representing the Document AST
     * @throws TometException if syntax parsing fails
     */
    public static native String parseDocumentJson(String source) throws TometException;

    /**
     * Parses a data-only .tmt document into a JSON string.
     *
     * @param source the data-only Tomet source text
     * @return JSON string of the parsed value
     * @throws TometException if syntax parsing fails
     */
    public static native String parseValueJson(String source) throws TometException;

    /**
     * Converts .tmt source text into an HTML body string.
     *
     * @param source the Tomet source text
     * @return rendered HTML body string
     * @throws TometException if syntax parsing fails
     */
    public static native String toHtml(String source) throws TometException;

    /**
     * Converts .tmt source text into a CommonMark Markdown string.
     *
     * @param source the Tomet source text
     * @return converted CommonMark Markdown string
     * @throws TometException if syntax parsing fails
     */
    public static native String toMarkdown(String source) throws TometException;

    /**
     * Converts .tmt source text into a Typst markup string.
     *
     * @param source the Tomet source text
     * @return converted Typst markup string
     * @throws TometException if syntax parsing fails
     */
    public static native String toTypst(String source) throws TometException;

    /**
     * Parses CommonMark Markdown text into a Document AST JSON string.
     *
     * @param markdown CommonMark source text
     * @return JSON string representing the Document AST
     * @throws TometException if parsing fails
     */
    public static native String fromMarkdownJson(String markdown) throws TometException;

    /**
     * Serializes a Document AST JSON string back into formatted .tmt source code.
     *
     * @param docJson JSON string representing the Document AST
     * @return formatted .tmt source text
     * @throws TometException if serialization fails
     */
    public static native String printDocumentJson(String docJson) throws TometException;

    /**
     * Formats .tmt source text with lossless whitespace hygiene and span preservation.
     *
     * @param source unformatted Tomet source text
     * @return cleanly formatted Tomet source text
     */
    public static native String format(String source);

    /**
     * Validates .tmt source text and returns a JSON array of validation errors.
     *
     * @param source Tomet source text
     * @return JSON string containing the array of validation diagnostics
     * @throws TometException if syntax parsing fails
     */
    public static native String validateJson(String source) throws TometException;

    /**
     * Validates .tmt source text against std plus the given vocabularies.
     *
     * <p>Without them, {@link #validateJson} knows only the std namespace, so
     * every element a vocabulary declares is reported as unknown. A vocabulary
     * source that does not parse, or that carries no {@code @vocabulary(ns)}
     * header, is skipped: it binds no namespace.
     *
     * @param source            Tomet source text
     * @param vocabulariesJson  JSON array of vocabulary source strings
     * @return JSON string containing the array of validation diagnostics
     * @throws TometException if syntax parsing fails
     */
    public static native String validateJsonWith(String source, String vocabulariesJson)
            throws TometException;
}
