#!/usr/bin/bash

docker-compose -f docker-compose.yml -f docker-compose.production.yml down

exit 0