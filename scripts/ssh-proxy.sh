EC2_HOST=ec2-user@$(terraform -chdir=./terraform output -raw http_proxy_ip)

ssh -A $EC2_HOST
