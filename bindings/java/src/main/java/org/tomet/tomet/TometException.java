package org.tomet.tomet;

/**
 * Exception thrown when a Tomet parsing, validation, or serialization error occurs.
 */
public class TometException extends RuntimeException {
    public TometException(String message) {
        super(message);
    }

    public TometException(String message, Throwable cause) {
        super(message, cause);
    }
}
