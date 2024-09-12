#!/bin/bash
OUTPUTDIR=$1
S3_BUCKET=$2

# sync gzippable files
s3cmd sync --progress --acl-public --add-header 'Cache-Control: max-age=1800' $OUTPUTDIR/ s3://$S3_BUCKET --exclude '*.*' --include '*.html'
s3cmd sync --progress --acl-public --add-header 'Cache-Control: max-age=43200' $OUTPUTDIR/ s3://$S3_BUCKET --exclude '*.*' --include '*.css' -m 'text/css'
s3cmd sync --progress --acl-public --add-header 'Cache-Control: max-age=43200' $OUTPUTDIR/ s3://$S3_BUCKET --exclude '*.*' --include '*.js' -m "text/javascript"

# sync non gzipped files
s3cmd sync --progress --acl-public $OUTPUTDIR/ s3://$S3_BUCKET --add-header 'Cache-Control: max-age=86400' --exclude '*.sh' --exclude '*.html' --exclude '*.js' --exclude '*.css'  --exclude '*.gz'
