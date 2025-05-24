EC2_HOST=admin@$(terraform -chdir=./terraform output -raw ec2_instance_ip)

ssh -A $EC2_HOST
