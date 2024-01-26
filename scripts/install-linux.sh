#!/usr/bin/env bash
set -eu

# get latest zip
repo="trdthg/t-autotest"

echo ">>> checking latest tag..."
tag=$(wget -q -O- https://api.github.com/repos/${repo}/releases/latest | jq -r '.tag_name')

# save dir
echo ">>> prepare dir"
user_dir=$HOME/.autotest
mkdir -p $user_dir

echo ">>> downloading zip..."
cd $user_dir
zip_name=autotest-linux.tar.gz
wget https://github.com/trdthg/t-autotest/releases/download/$tag/$zip_name -O autotest.tar.gz

echo "extracting..."
tar -zxvf autotest.tar.gz

cd --

# set env
echo ">>> setting env..."
echo 'export PATH=$PATH:$HOME/.autotest' >> ~/.bashrc
export PATH=$PATH:$HOME/.autotest

echo ">>> done!, you can try with 'autotest -v'"
