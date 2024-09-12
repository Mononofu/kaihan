# 開板 - kaihan

Custom static site generator used for https://www.furidamu.org/

For local testing, run:

```sh
python3 -m http.server 8787 --bind 127.0.0.1 --directory ~/tmp/rendered_blog/

RUST_LOG=info RUST_BACKTRACE=1 cargo run -- \
  --input ~/blog/ --output ~/tmp/rendered_blog/ \
  --siteurl http://localhost:8787
```

To publish, run:

```sh
RUST_LOG=info RUST_BACKTRACE=1 cargo run -- \
  --input ~/blog/ --output ~/tmp/rendered_blog/ \
./s3_upload.sh ~/tmp/rendered_blog/ www.furidamu.org
```