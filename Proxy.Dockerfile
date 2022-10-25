FROM envoyproxy/envoy:v1.24.0
RUN apt-get update && apt-get install -y ca-certificates
ENTRYPOINT /usr/local/bin/envoy -c /etc/envoy.yaml -l info --service-cluster proxy
