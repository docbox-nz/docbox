# Extract EC2 instance IP from the terraform output
EC2_HOST=admin@$(terraform -chdir=./terraform output -raw ec2_instance_ip)

REMOTE_LOG_PATH=/docbox.log
LOCAL_LOG_PATH=./private/docbox.log

# Dump the last 10 thousand log lines into a log file
ssh -A $EC2_HOST "sudo journalctl -u docbox -n 10000 | sudo tee $REMOTE_LOG_PATH > /dev/null"

# Download the log file to the local path
echo "Copying logs to local"
scp -A $EC2_HOST:$REMOTE_LOG_PATH $LOCAL_LOG_PATH
