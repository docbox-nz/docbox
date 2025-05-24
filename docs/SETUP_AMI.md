# Setup AMI

## Create EC2 instance

Go to the EC2 page, Launch an instance, selecting "Debian" as the base AMI
![alt text](images/image-6.png)

Setup a key pair for SSH access:

![alt text](images/image-7.png)

Select "Allow SSH traffic" and change the "Anywhere" to "My IP"

![alt text](images/image-8.png)

Follow normal instance creation steps. Ensure you setup an SSH key for accessing the machine

## Get SSH details

After creating the instance and after its started go to the instance and press "Connect":

![alt text](images/image-9.png)

Click the SSH client tab:

![alt text](images/image-10.png)

And copy the value under "Connect to your instance using its Public DNS:" you will use this as <instance-ssh-host> in the next steps

## Copy the ec2-initial-setup script

Copy the initial setup script to the EC2 instance:

```
scp -i "<path-to-created-ssh-key>" ./scripts/ec2-initial-setup.sh admin@<instance-ssh-host>:/tmp/ec2-initial-setup.sh
```

SSH into the instance:

```sh
ssh -i "<path-to-created-ssh-key>" admin@<instance-ssh-host>
```

Run the setup script as root:


```sh
sudo sh /tmp/ec2-initial-setup.sh
```

From the EC2 instance page on the website press the "Actions" dropdown

![alt text](images/image.png)

then "Images & Templates" then "Create image"

![alt text](images/image-1.png)

Fill out the image name and description (Rest can be left as is):

![alt text](images/image-2.png)

Create the image:

![alt text](images/image-3.png)

Wait for image to create:

![alt text](images/image-4.png)

Copy AMI ID

![alt text](images/image-5.png)


Use the AMI ID in the terraform variables