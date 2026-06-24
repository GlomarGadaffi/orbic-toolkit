#!/bin/sh
set -e
NAME={{name}}
BIN={{data_dir}}/{{binary_name}}
PIDFILE={{pidfile}}
LOGFILE={{log_file}}

case "$1" in
  start)
    echo -n "Starting $NAME: "
    {{pre_start}}start-stop-daemon -S -b --make-pidfile --pidfile $PIDFILE \
      --startas $BIN -- {{args}} >> $LOGFILE 2>&1
    echo "done"
    ;;
  stop)
    echo -n "Stopping $NAME: "
    start-stop-daemon -K -p $PIDFILE && echo "done" || echo "not running"
    ;;
  restart)
    $0 stop; sleep 1; $0 start
    ;;
  status)
    if start-stop-daemon -K --test -p $PIDFILE 2>/dev/null; then
      echo "$NAME is running"
    else
      echo "$NAME is not running"; exit 1
    fi
    ;;
  *)
    echo "Usage: $NAME { start | stop | restart | status }" >&2
    exit 1
    ;;
esac
exit 0
