FROM debian:stretch-slim

# install openssl
RUN apt-get update && apt-get install -y --no-install-recommends \
		openssl \
		\
&& rm -rf /var/lib/apt/lists/*

COPY target/x86_64-unknown-linux-gnu/release/rocket_aws_s3_proxy /usr/bin/rocket_aws_s3_proxy

EXPOSE 8080

# ENTRYPOINT [ "/usr/bin/rocket_aws_s3_proxy" ]
