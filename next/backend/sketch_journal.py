"""Accepted-action journal callback client. Storage and decisions live in Rust."""
import session_callbacks as Callbacks

CHANNEL = Callbacks.JOURNAL
JournalError = Callbacks.CallbackError
configure = Callbacks.configure
request = Callbacks.request


def call(action, document, **parameters):
    return Callbacks.call(CHANNEL, action, document, **parameters)
