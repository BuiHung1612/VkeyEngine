#!/bin/sh
# Arguments: old process id, installed bundle, staged bundle.
while kill -0 "$1" 2>/dev/null; do sleep 0.2; done
rm -rf "$2.old" || exit 1
if ! mv "$2" "$2.old"; then
    open "$2"
    exit 1
fi
if ! mv "$3" "$2"; then
    mv "$2.old" "$2"
    open "$2"
    exit 1
fi
if ! open "$2"; then
    rm -rf "$2" && mv "$2.old" "$2"
    open "$2"
    exit 1
fi
# The new process removes the backup after it has been running for 10 seconds.
