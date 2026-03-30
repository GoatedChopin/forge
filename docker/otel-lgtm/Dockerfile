FROM grafana/otel-lgtm:0.20.0

# Remove default JVM/RED dashboards that ship with the base image
RUN rm -f /otel-lgtm/grafana/conf/provisioning/dashboards/grafana-dashboards.yaml \
          /otel-lgtm/grafana-dashboard-red-metrics-classic.json \
          /otel-lgtm/grafana-dashboard-red-metrics-native.json \
          /otel-lgtm/grafana-dashboard-jvm-metrics.json

COPY provisioning.yaml /otel-lgtm/grafana/conf/provisioning/dashboards/forge.yaml
COPY datasources/ /otel-lgtm/grafana/conf/provisioning/datasources/
COPY dashboards/ /var/lib/grafana/dashboards/forge/
